// `Image`: network images through the runtime's image loader (downsampled to
// the view size), local files, `data:` URIs. Drawn as layer contents so every
// content mode (aspect fill included) is a contents gravity.

import AppKit
import ImageIO

final class NSHImageView: NSHComponentView {
    let imageLayer = CALayer()
    private var source: String?
    private var tint: NSColor?
    private var loadToken = 0
    private(set) var loaded: CGImage?

    override func commonInit() {
        imageLayer.contentsGravity = .resizeAspectFill
        imageLayer.masksToBounds = true
        layer?.addSublayer(imageLayer)
    }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        imageLayer.frame = bounds
        imageLayer.cornerRadius = layer?.cornerRadius ?? 0
        imageLayer.cornerCurve = layer?.cornerCurve ?? .continuous
        imageLayer.contentsScale = window?.backingScaleFactor ?? TextMeasurer.scale
        CATransaction.commit()
        if source != nil, loaded == nil, bounds.width > 0 { load() }
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .imageSource:
            let s = value.string
            guard s != source else { return true }
            source = s
            loaded = nil
            imageLayer.contents = nil
            load()
        case .contentMode:
            switch value.int ?? 0 {
            case 1: imageLayer.contentsGravity = .resizeAspect
            case 2: imageLayer.contentsGravity = .resize
            case 3: imageLayer.contentsGravity = .center
            default: imageLayer.contentsGravity = .resizeAspectFill
            }
        case .tintColor:
            tint = value.nsColor
            show(loaded)
        default:
            return super.applyProp(prop, value)
        }
        return true
    }

    private func load() {
        loadToken += 1
        let token = loadToken
        guard let s = source, !s.isEmpty else { return }
        let size = bounds.size
        let scale = TextMeasurer.scale
        let deliver: (CGImage?) -> Void = { [weak self] image in
            let apply = {
                guard let self = self, self.loadToken == token else { return }
                self.loaded = image
                self.show(image)
            }
            if Thread.isMainThread { apply() } else { DispatchQueue.main.async(execute: apply) }
        }
        if s.hasPrefix("http://") || s.hasPrefix("https://"), let url = URL(string: s) {
            guard size.width > 0, size.height > 0 else { return }
            let runtime = context?.runtime ?? NSHRuntime.shared
            runtime.loadImage(url, size) { deliver($0?.cgImage(forProposedRect: nil, context: nil, hints: nil)) }
        } else if s.hasPrefix("data:") {
            DispatchQueue.global(qos: .userInitiated).async {
                guard let comma = s.firstIndex(of: ","), let data = Data(base64Encoded: String(s[s.index(after: comma)...])) else {
                    return deliver(nil)
                }
                deliver(ImageDecoding.downsample(data, to: size, scale: scale))
            }
        } else {
            // Local images arrive as absolute paths (the core resolves bundle-relative
            // ones); anything else is never resolved against the process's cwd.
            let path = s.hasPrefix("file://") ? (URL(string: s)?.path ?? "") : s
            guard path.hasPrefix("/") else {
                context?.runtime.log(.warn, "native-shell: image source `\(s)` is not an absolute path or URL")
                return deliver(nil)
            }
            DispatchQueue.global(qos: .userInitiated).async {
                guard let data = FileManager.default.contents(atPath: path) else { return deliver(nil) }
                deliver(ImageDecoding.downsample(data, to: size, scale: scale))
            }
        }
    }

    private func show(_ image: CGImage?) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if let image = image, let tint = tint {
            imageLayer.contents = ImageDecoding.tinted(image, with: tint)
        } else {
            imageLayer.contents = image
        }
        CATransaction.commit()
    }

    override func resetForReuse() {
        super.resetForReuse()
        loadToken += 1
        loaded = nil
        imageLayer.contents = nil
    }
}

enum ImageDecoding {
    /// Decodes at most the pixels the view can show.
    static func downsample(_ data: Data, to size: CGSize, scale: CGFloat) -> CGImage? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let src = CGImageSourceCreateWithData(data as CFData, options) else { return nil }
        let maxPixel = max(size.width, size.height) * scale
        guard maxPixel > 0 else { return CGImageSourceCreateImageAtIndex(src, 0, nil) }
        let thumbOptions = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        ] as CFDictionary
        return CGImageSourceCreateThumbnailAtIndex(src, 0, thumbOptions)
    }

    /// The image's alpha filled with `color` (template rendering).
    static func tinted(_ image: CGImage, with color: NSColor) -> CGImage? {
        let w = image.width, h = image.height
        guard let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        let r = CGRect(x: 0, y: 0, width: w, height: h)
        ctx.clip(to: r, mask: image)
        ctx.setFillColor(color.cgColor)
        ctx.fill(r)
        return ctx.makeImage()
    }
}

/// The default image loader: URLSession with an in-memory cache of decoded,
/// downsampled images.
final class DefaultImageLoader {
    static let shared = DefaultImageLoader()
    private let cache = NSCache<NSString, NSImage>()
    private let session: URLSession = {
        let c = URLSessionConfiguration.default
        c.urlCache = URLCache(memoryCapacity: 8 << 20, diskCapacity: 64 << 20)
        return URLSession(configuration: c)
    }()

    init() { cache.totalCostLimit = 64 << 20 }

    func load(_ url: URL, size: CGSize, completion: @escaping (NSImage?) -> Void) {
        let scale = TextMeasurer.scale
        let key = "\(url.absoluteString)|\(Int(size.width * scale))x\(Int(size.height * scale))" as NSString
        if let hit = cache.object(forKey: key) { return completion(hit) }
        session.dataTask(with: url) { [cache] data, _, _ in
            guard let data = data, let cg = ImageDecoding.downsample(data, to: size, scale: scale) else {
                return completion(nil)
            }
            let image = NSImage(cgImage: cg, size: CGSize(width: CGFloat(cg.width) / scale, height: CGFloat(cg.height) / scale))
            cache.setObject(image, forKey: key, cost: cg.width * cg.height * 4)
            completion(image)
        }.resume()
    }
}
