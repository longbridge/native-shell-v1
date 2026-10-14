import Foundation
import NativeShellShared
import XCTest

final class ProgramDirectoryTests: XCTestCase {
    func testTextAsIsBinaryAsDataURLsHiddenAndNodeModulesSkipped() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("nsh-program-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let fm = FileManager.default
        for sub in ["assets", "lib", "node_modules/pkg", ".git"] {
            try fm.createDirectory(at: dir.appendingPathComponent(sub), withIntermediateDirectories: true)
        }
        try Data(#"{"id": "dev.test.dir", "name": "Dir"}"#.utf8).write(to: dir.appendingPathComponent("native-shell.json"))
        try Data("export default 1;".utf8).write(to: dir.appendingPathComponent("main.js"))
        try Data("export const x = 'é';".utf8).write(to: dir.appendingPathComponent("lib/x.js"))
        let jpeg = Data([0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0xFF, 0xD9])
        try jpeg.write(to: dir.appendingPathComponent("assets/route.JPG"))
        try Data([0x00, 0xFF, 0xFE]).write(to: dir.appendingPathComponent("assets/blob.bin"))
        for f in ["node_modules/pkg/index.js", ".DS_Store", ".git/HEAD"] {
            try Data("ignored".utf8).write(to: dir.appendingPathComponent(f))
        }

        let json = try XCTUnwrap(JSON.object(ProgramDirectory.sourcesJSON(dir)))
        XCTAssertEqual(json.object("manifest")?.string("id"), "dev.test.dir")
        let files = try XCTUnwrap(json["files"] as? [String: String])
        XCTAssertEqual(Set(files.keys), ["main.js", "lib/x.js", "assets/route.JPG", "assets/blob.bin"])
        XCTAssertEqual(files["lib/x.js"], "export const x = 'é';")
        XCTAssertEqual(files["assets/route.JPG"], "data:image/jpeg;base64,\(jpeg.base64EncodedString())")
        XCTAssertTrue(files["assets/blob.bin"]?.hasPrefix("data:application/octet-stream;base64,") ?? false)
    }

    func testMissingManifestThrows() {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("nsh-empty-\(UUID().uuidString)")
        XCTAssertThrowsError(try ProgramDirectory.sourcesJSON(dir))
    }
}
