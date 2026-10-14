// `Image`: network images through the runtime's image loader (downsampled to
// the view size), local files, `data:` URIs.

import NativeShellShared
import ImageIO
import UIKit

final class NSHImageView: NSHComponentView {
    let imageView = UIImageView()
    private var source: String?
    private var tint: UIColor?
    private var loadToken = 0
    private var loaded: UIImage?

    override func commonInit() {
        imageView.contentMode = .scaleAspectFill
        imageView.clipsToBounds = true
        addSubview(imageView)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        imageView.frame = bounds
        if source != nil, loaded == nil, bounds.width > 0 { load() }
    }

    override func applyProp(_ prop: Prop, _ value: PropValue) -> Bool {
        switch prop {
        case .imageSource:
            let s = value.string
            guard s != source else { return true }
            source = s
            loaded = nil
            imageView.image = nil
            load()
        case .contentMode:
            switch value.int ?? 0 {
            case 1: imageView.contentMode = .scaleAspectFit
            case 2: imageView.contentMode = .scaleToFill
            case 3: imageView.contentMode = .center
            default: imageView.contentMode = .scaleAspectFill
            }
        case .tintColor:
            tint = value.uiColor
            imageView.tintColor = tint
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
        let deliver: (UIImage?) -> Void = { [weak self] image in
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
            runtime.loadImage(url, size, deliver)
        } else if s.hasPrefix("data:") {
            DispatchQueue.global(qos: .userInitiated).async {
                guard let comma = s.firstIndex(of: ","), let data = Data(base64Encoded: String(s[s.index(after: comma)...])) else {
                    return deliver(nil)
                }
                deliver(ImageDecoding.downsample(data, to: size, scale: TextMeasurer.scale))
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
                deliver(ImageDecoding.downsample(data, to: size, scale: TextMeasurer.scale))
            }
        }
    }

    private func show(_ image: UIImage?) {
        imageView.image = tint != nil ? image?.withRenderingMode(.alwaysTemplate) : image
    }

    override func resetForReuse() {
        super.resetForReuse()
        loadToken += 1
        loaded = nil
        imageView.image = nil
    }
}

enum ImageDecoding {
    /// Decodes at most the pixels the view can show.
    static func downsample(_ data: Data, to size: CGSize, scale: CGFloat) -> UIImage? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let src = CGImageSourceCreateWithData(data as CFData, options) else { return nil }
        let maxPixel = max(size.width, size.height) * scale
        guard maxPixel > 0 else { return UIImage(data: data) }
        let thumbOptions = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
        ] as CFDictionary
        guard let cg = CGImageSourceCreateThumbnailAtIndex(src, 0, thumbOptions) else { return nil }
        return UIImage(cgImage: cg, scale: scale, orientation: .up)
    }
}

/// The default image loader: URLSession with an in-memory cache of decoded,
/// downsampled images.
final class DefaultImageLoader {
    static let shared = DefaultImageLoader()
    private let cache = NSCache<NSString, UIImage>()
    private let session: URLSession = {
        let c = URLSessionConfiguration.default
        c.urlCache = URLCache(memoryCapacity: 8 << 20, diskCapacity: 64 << 20)
        return URLSession(configuration: c)
    }()

    init() { cache.totalCostLimit = 64 << 20 }

    func load(_ url: URL, size: CGSize, completion: @escaping (UIImage?) -> Void) {
        let scale = TextMeasurer.scale
        let key = "\(url.absoluteString)|\(Int(size.width * scale))x\(Int(size.height * scale))" as NSString
        if let hit = cache.object(forKey: key) { return completion(hit) }
        session.dataTask(with: url) { [cache] data, _, _ in
            guard let data = data, let image = ImageDecoding.downsample(data, to: size, scale: scale) else {
                return completion(nil)
            }
            let cost = Int(image.size.width * image.size.height * image.scale * image.scale * 4)
            cache.setObject(image, forKey: key, cost: cost)
            completion(image)
        }.resume()
    }
}
