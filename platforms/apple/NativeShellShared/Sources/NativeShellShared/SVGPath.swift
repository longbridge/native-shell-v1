// SVG path data to CGPath, shared by the renderers' `Shape` views. Path data
// comes from scripts: anything malformed ends the path (or yields nil), never
// a crash.

import CoreGraphics

/// SVG path data: M L H V C S Q T A Z, absolute and relative.
public enum SVGPath {
    public static func parse(_ d: String) -> CGPath? {
        var tokens = Tokenizer(d)
        let path = CGMutablePath()
        var current = CGPoint.zero, start = CGPoint.zero
        var lastControl: CGPoint?
        var lastCommand: Character = " "
        var command: Character?
        while true {
            if let c = tokens.command() {
                command = c
            } else if tokens.atEnd {
                break
            } else if command == nil {
                return nil
            }
            guard let cmd = command else { break }
            let rel = cmd.isLowercase
            let base = rel ? current : .zero
            func pt() -> CGPoint? {
                guard let x = tokens.number(), let y = tokens.number() else { return nil }
                return CGPoint(x: base.x + x, y: base.y + y)
            }
            switch Character(cmd.uppercased()) {
            case "M":
                guard let p = pt() else { return path.isEmpty ? nil : path }
                path.move(to: p)
                current = p; start = p
                command = rel ? "l" : "L" // subsequent pairs are line-tos
            case "L":
                guard let p = pt() else { return path }
                path.addLine(to: p); current = p
            case "H":
                guard let x = tokens.number() else { return path }
                current = CGPoint(x: (rel ? current.x : 0) + x, y: current.y); path.addLine(to: current)
            case "V":
                guard let y = tokens.number() else { return path }
                current = CGPoint(x: current.x, y: (rel ? current.y : 0) + y); path.addLine(to: current)
            case "C":
                guard let c1 = pt(), let c2 = pt(), let p = pt() else { return path }
                path.addCurve(to: p, control1: c1, control2: c2)
                lastControl = c2; current = p
            case "S":
                guard let c2 = pt(), let p = pt() else { return path }
                let c1 = "CScs".contains(lastCommand) ? reflect(lastControl, current) : current
                path.addCurve(to: p, control1: c1, control2: c2)
                lastControl = c2; current = p
            case "Q":
                guard let c = pt(), let p = pt() else { return path }
                path.addQuadCurve(to: p, control: c)
                lastControl = c; current = p
            case "T":
                guard let p = pt() else { return path }
                let c = "QTqt".contains(lastCommand) ? reflect(lastControl, current) : current
                path.addQuadCurve(to: p, control: c)
                lastControl = c; current = p
            case "A":
                guard let rx = tokens.number(), let ry = tokens.number(), let rot = tokens.number(),
                      let large = tokens.flag(), let sweep = tokens.flag(), let p = pt() else { return path }
                arc(path, from: current, to: p, rx: rx, ry: ry, rotation: rot, large: large, sweep: sweep)
                current = p
            case "Z":
                path.closeSubpath()
                current = start
                command = nil
            default:
                return nil
            }
            lastCommand = cmd
            if cmd == "Z" || cmd == "z" { lastCommand = cmd }
        }
        return path
    }

    private static func reflect(_ c: CGPoint?, _ p: CGPoint) -> CGPoint {
        guard let c = c else { return p }
        return CGPoint(x: 2 * p.x - c.x, y: 2 * p.y - c.y)
    }

    /// SVG elliptical arc (endpoint parameterization) as cubic béziers.
    private static func arc(_ path: CGMutablePath, from p0: CGPoint, to p1: CGPoint, rx rx0: CGFloat, ry ry0: CGFloat,
                            rotation: CGFloat, large: Bool, sweep: Bool) {
        var rx = abs(rx0), ry = abs(ry0)
        if rx == 0 || ry == 0 || p0 == p1 { path.addLine(to: p1); return }
        let phi = rotation * .pi / 180
        let cosP = cos(phi), sinP = sin(phi)
        let dx = (p0.x - p1.x) / 2, dy = (p0.y - p1.y) / 2
        let x1 = cosP * dx + sinP * dy, y1 = -sinP * dx + cosP * dy
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry)
        if lambda > 1 { rx *= sqrt(lambda); ry *= sqrt(lambda) }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1
        var coef = den == 0 ? 0 : sqrt(max(0, num / den))
        if large == sweep { coef = -coef }
        let cx1 = coef * rx * y1 / ry, cy1 = -coef * ry * x1 / rx
        let cx = cosP * cx1 - sinP * cy1 + (p0.x + p1.x) / 2
        let cy = sinP * cx1 + cosP * cy1 + (p0.y + p1.y) / 2
        func angle(_ ux: CGFloat, _ uy: CGFloat, _ vx: CGFloat, _ vy: CGFloat) -> CGFloat {
            let a = atan2(ux * vy - uy * vx, ux * vx + uy * vy)
            return a
        }
        let theta1 = angle(1, 0, (x1 - cx1) / rx, (y1 - cy1) / ry)
        var delta = angle((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry)
        if !sweep, delta > 0 { delta -= 2 * .pi }
        if sweep, delta < 0 { delta += 2 * .pi }
        let segments = max(1, Int(ceil(abs(delta) / (.pi / 2))))
        let step = delta / CGFloat(segments)
        var t = theta1
        for _ in 0 ..< segments {
            let k = 4 / 3 * tan(step / 4)
            let c1 = CGPoint(x: cos(t) - k * sin(t), y: sin(t) + k * cos(t))
            let e = CGPoint(x: cos(t + step), y: sin(t + step))
            let c2 = CGPoint(x: e.x + k * sin(t + step), y: e.y - k * cos(t + step))
            func map(_ p: CGPoint) -> CGPoint {
                CGPoint(x: cx + rx * p.x * cosP - ry * p.y * sinP, y: cy + rx * p.x * sinP + ry * p.y * cosP)
            }
            path.addCurve(to: map(e), control1: map(c1), control2: map(c2))
            t += step
        }
    }

    private struct Tokenizer {
        let chars: [Character]
        var i = 0

        init(_ s: String) { chars = Array(s) }

        mutating func skip() {
            while i < chars.count, chars[i].isWhitespace || chars[i] == "," { i += 1 }
        }

        var atEnd: Bool {
            mutating get { skip(); return i >= chars.count }
        }

        mutating func command() -> Character? {
            skip()
            guard i < chars.count, "MLHVCSQTAZmlhvcsqtaz".contains(chars[i]) else { return nil }
            defer { i += 1 }
            return chars[i]
        }

        mutating func flag() -> Bool? {
            skip()
            guard i < chars.count, chars[i] == "0" || chars[i] == "1" else { return nil }
            defer { i += 1 }
            return chars[i] == "1"
        }

        mutating func number() -> CGFloat? {
            skip()
            let begin = i
            if i < chars.count, chars[i] == "-" || chars[i] == "+" { i += 1 }
            var seenDot = false, seenDigit = false
            while i < chars.count {
                let c = chars[i]
                if c.isASCII, c.isNumber { seenDigit = true; i += 1 }
                else if c == ".", !seenDot { seenDot = true; i += 1 }
                else if (c == "e" || c == "E"), seenDigit {
                    i += 1
                    if i < chars.count, chars[i] == "-" || chars[i] == "+" { i += 1 }
                } else { break }
            }
            guard seenDigit, let v = Double(String(chars[begin ..< i])) else { i = begin; return nil }
            return CGFloat(v)
        }
    }
}
