// Offline HUD artwork authoring. Run from the repository root with Swift on macOS.
// Writes only the reserved (64,64)..(128,128) cell; runtime loads the committed PNG.
import AppKit
import Foundation

let url = URL(fileURLWithPath: "assets/shared/ui/font_01.png")
let original = try Data(contentsOf: url)
guard let atlas = NSBitmapImageRep(data: original), atlas.pixelsWide == 256, atlas.pixelsHigh == 128,
      let icon = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 64,
          bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
          colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
      let context = NSGraphicsContext(bitmapImageRep: icon) else {
    throw NSError(domain: "HealthIcon", code: 1)
}
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
let heart = NSBezierPath()
// AppKit y points upward. The point is low; both lobes are high and symmetric.
heart.move(to: NSPoint(x: 32, y: 5))
heart.curve(to: NSPoint(x: 5, y: 40), controlPoint1: NSPoint(x: 26, y: 13), controlPoint2: NSPoint(x: 5, y: 28))
heart.curve(to: NSPoint(x: 32, y: 48), controlPoint1: NSPoint(x: 5, y: 60), controlPoint2: NSPoint(x: 25, y: 63))
heart.curve(to: NSPoint(x: 59, y: 40), controlPoint1: NSPoint(x: 39, y: 63), controlPoint2: NSPoint(x: 59, y: 60))
heart.curve(to: NSPoint(x: 32, y: 5), controlPoint1: NSPoint(x: 59, y: 28), controlPoint2: NSPoint(x: 38, y: 13))
heart.close()
NSColor.white.setFill()
heart.fill()
NSGraphicsContext.restoreGraphicsState()
for y in 0..<64 {
    for x in 0..<64 {
        var pixel = [UInt](repeating: 0, count: 4)
        icon.getPixel(&pixel, atX: x, y: y)
        atlas.setPixel(&pixel, atX: x + 64, y: y + 64)
    }
}
guard let result = atlas.representation(using: .png, properties: [:]) else {
    throw NSError(domain: "HealthIcon", code: 2)
}
try result.write(to: url, options: .atomic)
// The editor uses its own glyph atlas. Its unused bottom-left 64x64 cell
// carries exactly the same heart; every existing ASCII pixel stays intact.
let editorURL = URL(fileURLWithPath: "assets/shared/ui/editor_font.png")
let editorData = try Data(contentsOf: editorURL)
guard let editorAtlas = NSBitmapImageRep(data: editorData),
      editorAtlas.pixelsWide == 512, editorAtlas.pixelsHigh == 448 else {
    throw NSError(domain: "HealthIcon", code: 3)
}
for y in 0..<64 {
    for x in 0..<64 {
        var pixel = [UInt](repeating: 0, count: 4)
        icon.getPixel(&pixel, atX: x, y: y)
        editorAtlas.setPixel(&pixel, atX: x, y: y + 384)
    }
}
guard let editorResult = editorAtlas.representation(using: .png, properties: [:]) else {
    throw NSError(domain: "HealthIcon", code: 4)
}
try editorResult.write(to: editorURL, options: .atomic)
