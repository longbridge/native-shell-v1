// A program directory (`native-shell.json` and its files) as the `{manifest,
// files}` JSON `nsh_instance_load_sources` takes. Text files go in as they
// are; anything that is not UTF-8 (images, fonts) as a `data:<mime>;base64,…`
// URL, as `native-shell dev` sends them, so program-relative assets such as
// `image("assets/photo.jpg")` resolve.

import Foundation

public enum ProgramDirectory {
    /// Hidden entries and `node_modules` are skipped.
    public static func sourcesJSON(_ directory: URL) throws -> String {
        let manifestURL = directory.appendingPathComponent("native-shell.json")
        let manifest = try JSONSerialization.jsonObject(with: Data(contentsOf: manifestURL))
        var files: [String: String] = [:]
        let base = directory.standardizedFileURL.path
        let e = FileManager.default.enumerator(at: directory, includingPropertiesForKeys: [.isRegularFileKey], options: [.skipsHiddenFiles])
        while let url = e?.nextObject() as? URL {
            if url.lastPathComponent == "node_modules" {
                e?.skipDescendants()
                continue
            }
            guard (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true,
                  url.lastPathComponent != "native-shell.json" else { continue }
            let path = url.standardizedFileURL.path
            guard path.hasPrefix(base + "/") else { continue }
            let data = try Data(contentsOf: url)
            files[String(path.dropFirst(base.count + 1))] = String(data: data, encoding: .utf8)
                ?? "data:\(mimeType(url.pathExtension));base64,\(data.base64EncodedString())"
        }
        return JSON.string(["manifest": manifest, "files": files])
    }

    /// The same table as the CLI's; the engine only needs the base64 payload.
    public static func mimeType(_ ext: String) -> String {
        switch ext.lowercased() {
        case "png": return "image/png"
        case "jpg", "jpeg": return "image/jpeg"
        case "gif": return "image/gif"
        case "webp": return "image/webp"
        case "svg": return "image/svg+xml"
        case "ttf": return "font/ttf"
        case "otf": return "font/otf"
        case "woff2": return "font/woff2"
        case "json": return "application/json"
        default: return "application/octet-stream"
        }
    }
}
