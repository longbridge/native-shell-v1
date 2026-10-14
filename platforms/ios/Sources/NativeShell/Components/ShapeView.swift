// `Shape`: SVG path data drawn by a CAShapeLayer, scaled from its view box to
// the bounds like SVG's default `xMidYMid meet`.

import NativeShellShared
import UIKit

final class NSHShapeView: NSHComponentView {
    private let shape = CAShapeLayer()
    private var path: CGPath?
    private var viewBox: CGRect?
    private var strokeWidth: CGFloat = 1

    override func commonInit() {
        shape.fillColor = nil
        shape.strokeColor = nil
        // Round caps and joins everywhere: icon sets draw dots as zero-length segments.
        shape.lineCap = .round
        shape.lineJoin = .round
        layer.addSublayer(shape)
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .pathData: path = value.string.flatMap(SVGPath.parse)
        case .fillColor: shape.fillColor = value.uiColor?.cgColor
        case .strokeColor: shape.strokeColor = value.uiColor?.cgColor
        case .strokeWidth: strokeWidth = value.number.map { CGFloat($0) } ?? 1
        case .fillRule: shape.fillRule = value.int == 1 ? .evenOdd : .nonZero
        case .viewBox:
            let a = value.floats ?? []
            viewBox = a.count == 4 && a[2] > 0 && a[3] > 0 ? CGRect(x: CGFloat(a[0]), y: CGFloat(a[1]), width: CGFloat(a[2]), height: CGFloat(a[3])) : nil
        default: return super.applyProp(prop, value)
        }
        redraw()
        return true
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        redraw()
    }

    private func redraw() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        shape.frame = bounds
        guard let p = path else { shape.path = nil; return }
        let box = viewBox ?? p.boundingBoxOfPath
        var scale: CGFloat = 1
        var t = CGAffineTransform.identity
        if box.width > 0, box.height > 0, bounds.width > 0, bounds.height > 0 {
            scale = min(bounds.width / box.width, bounds.height / box.height)
            let dx = (bounds.width - box.width * scale) / 2, dy = (bounds.height - box.height * scale) / 2
            t = CGAffineTransform(translationX: dx, y: dy).scaledBy(x: scale, y: scale).translatedBy(x: -box.minX, y: -box.minY)
        }
        shape.path = p.copy(using: &t)
        shape.lineWidth = strokeWidth * scale
    }

    /// The path in view coordinates (tests).
    var renderedPath: CGPath? { shape.path }
}
