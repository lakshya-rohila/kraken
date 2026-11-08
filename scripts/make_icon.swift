// Renders the Kraken app icon: "the cursor is watching".
// A kraken surfaces from the abyss; its one huge eye has a slit pupil shaped
// like a terminal block cursor. Usage: swift make_icon.swift out.png [px]
//
// Full-bleed square artwork (no rounded corners, no transparent margin):
// current macOS applies the icon mask itself, per Apple's HIG.
import AppKit

let px: CGFloat = CommandLine.arguments.count > 2 ? CGFloat(Double(CommandLine.arguments[2]) ?? 2048) : 2048
let S: CGFloat = 1024 // design units; scaled to `px`
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(px), pixelsHigh: Int(px), bitsPerSample: 8,
                           samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                           bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let ctx = NSGraphicsContext.current!.cgContext
ctx.scaleBy(x: px / S, y: px / S)
ctx.setShouldAntialias(true)
ctx.interpolationQuality = .high

let space = CGColorSpace(name: CGColorSpace.sRGB)!
func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(colorSpace: space, components: [CGFloat((hex >> 16) & 0xff) / 255, CGFloat((hex >> 8) & 0xff) / 255, CGFloat(hex & 0xff) / 255, a])!
}
func gradient(_ stops: [(UInt32, CGFloat, CGFloat)]) -> CGGradient {
    CGGradient(colorsSpace: space, colors: stops.map { rgb($0.0, $0.2) } as CFArray, locations: stops.map { $0.1 })!
}
func clipped(_ p: CGPath, _ draw: () -> Void) {
    ctx.saveGState(); ctx.addPath(p); ctx.clip(); draw(); ctx.restoreGState()
}

// ── Palette ──────────────────────────────────────────────────────────────
let abyssTop: UInt32 = 0x0B1026, abyssBottom: UInt32 = 0x04050A
let bruiseDark: UInt32 = 0x221552, bruise: UInt32 = 0x5B3FD0, bruiseLit: UInt32 = 0x8B6CFF
let biolume: UInt32 = 0x3DE0F5
let ember: UInt32 = 0xB3120F, blood: UInt32 = 0xFF3A18, amber: UInt32 = 0xFFD678

// ── Background: the abyss, with light rising from the deep ───────────────
let full = CGRect(x: 0, y: 0, width: S, height: S)
ctx.drawLinearGradient(gradient([(abyssBottom, 0, 1), (abyssTop, 1, 1)]), start: CGPoint(x: 0, y: 0), end: CGPoint(x: 0, y: S), options: [])
ctx.drawRadialGradient(gradient([(biolume, 0, 0.30), (biolume, 1, 0)]), startCenter: CGPoint(x: 512, y: -40), startRadius: 0,
                       endCenter: CGPoint(x: 512, y: -40), endRadius: 620, options: [])
// A faint violet halo behind the creature.
ctx.drawRadialGradient(gradient([(bruise, 0, 0.28), (bruise, 1, 0)]), startCenter: CGPoint(x: 512, y: 600), startRadius: 0,
                       endCenter: CGPoint(x: 512, y: 600), endRadius: 520, options: [])
_ = full

// ── Tentacles: tapered bodies built from left/right edges along a curve ──
func bezier(_ t: CGFloat, _ p: [CGPoint]) -> CGPoint {
    let u = 1 - t
    return CGPoint(x: u*u*u*p[0].x + 3*u*u*t*p[1].x + 3*u*t*t*p[2].x + t*t*t*p[3].x,
                   y: u*u*u*p[0].y + 3*u*u*t*p[1].y + 3*u*t*t*p[2].y + t*t*t*p[3].y)
}
/// Spine = a cubic curve, then a curl (spiral) at the tip.
func spine(_ ctrl: [CGPoint], curl: CGFloat, curlDir: CGFloat) -> [CGPoint] {
    var pts = (0...90).map { bezier(CGFloat($0) / 90, ctrl) }
    let a = pts[pts.count - 2], b = pts[pts.count - 1]
    var heading = atan2(b.y - a.y, b.x - a.x)
    var step = hypot(b.x - a.x, b.y - a.y)
    var p = b
    for _ in 0..<70 { // the tip winds into a tightening curl
        heading += curl * curlDir
        step *= 0.965
        p = CGPoint(x: p.x + cos(heading) * step, y: p.y + sin(heading) * step)
        pts.append(p)
    }
    return pts
}
func tentacle(_ pts: [CGPoint], base: CGFloat, tip: CGFloat) -> (CGPath, [(CGPoint, CGFloat)]) {
    var left: [CGPoint] = [], right: [CGPoint] = [], suckers: [(CGPoint, CGFloat)] = []
    for i in 0..<pts.count {
        let t = CGFloat(i) / CGFloat(pts.count - 1)
        let w = base * pow(1 - t, 1.15) + tip * t
        let a = pts[max(i - 1, 0)], b = pts[min(i + 1, pts.count - 1)]
        let ang = atan2(b.y - a.y, b.x - a.x) + .pi / 2
        let n = CGPoint(x: cos(ang), y: sin(ang))
        left.append(CGPoint(x: pts[i].x + n.x * w / 2, y: pts[i].y + n.y * w / 2))
        right.append(CGPoint(x: pts[i].x - n.x * w / 2, y: pts[i].y - n.y * w / 2))
        if i % 11 == 6 && t < 0.78 { // suckers along the underside
            suckers.append((CGPoint(x: pts[i].x - n.x * w * 0.26, y: pts[i].y - n.y * w * 0.26), w * 0.15))
        }
    }
    let path = CGMutablePath()
    path.move(to: left[0])
    for p in left.dropFirst() { path.addLine(to: p) }
    let tipEnd = pts.last!
    path.addLine(to: tipEnd)
    for p in right.reversed() { path.addLine(to: p) }
    path.closeSubpath()
    return (path, suckers)
}
func drawTentacle(_ pts: [CGPoint], base: CGFloat, dark: Bool) {
    let (path, suckers) = tentacle(pts, base: base, tip: 3)
    // Deep-water silhouette shadow first, for separation.
    ctx.saveGState()
    ctx.setShadow(offset: .zero, blur: 18, color: rgb(0x000000, 0.65))
    ctx.addPath(path); ctx.setFillColor(rgb(bruiseDark)); ctx.fillPath()
    ctx.restoreGState()
    clipped(path) {
        // Lit from below (the abyss glow), shadowed above.
        let g = dark ? gradient([(0x4A2A9A, 0, 1), (0x2C1A6A, 0.45, 1), (0x120A30, 1, 1)])
                     : gradient([(0x8A3FD6, 0, 1), (0x5A38C2, 0.35, 1), (0x33227E, 0.7, 1), (0x150C38, 1, 1)])
        ctx.drawLinearGradient(g, start: CGPoint(x: 0, y: 40), end: CGPoint(x: 0, y: 640), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    }
    for (c, r) in suckers {
        ctx.setFillColor(rgb(0xD9A0FF, dark ? 0.16 : 0.3))
        ctx.fillEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2))
        ctx.setFillColor(rgb(0x2A0F4A, 0.75))
        let r2 = r * 0.45
        ctx.fillEllipse(in: CGRect(x: c.x - r2, y: c.y - r2, width: r2 * 2, height: r2 * 2))
    }
}

// Back tentacles (darker, behind), then the front ones.
let back: [([CGPoint], CGFloat, CGFloat)] = [
    ([CGPoint(x: 430, y: 430), CGPoint(x: 360, y: 300), CGPoint(x: 250, y: 260), CGPoint(x: 150, y: 150)], 0.12, -1),
    ([CGPoint(x: 594, y: 430), CGPoint(x: 664, y: 300), CGPoint(x: 774, y: 260), CGPoint(x: 874, y: 150)], 0.12, 1),
]
for (c, curl, dir) in back { drawTentacle(spine(c, curl: curl, curlDir: dir), base: 100, dark: true) }

// Two that climb the sides, gripping the frame.
let climbers: [([CGPoint], CGFloat, CGFloat)] = [
    ([CGPoint(x: 360, y: 470), CGPoint(x: 140, y: 430), CGPoint(x: 60, y: 640), CGPoint(x: 170, y: 840)], 0.17, -1),
    ([CGPoint(x: 664, y: 470), CGPoint(x: 884, y: 430), CGPoint(x: 964, y: 640), CGPoint(x: 854, y: 840)], 0.17, 1),
]
for (c, curl, dir) in climbers { drawTentacle(spine(c, curl: curl, curlDir: dir), base: 104, dark: false) }

let front: [([CGPoint], CGFloat, CGFloat)] = [
    ([CGPoint(x: 470, y: 420), CGPoint(x: 450, y: 250), CGPoint(x: 360, y: 150), CGPoint(x: 330, y: 60)], 0.14, 1),
    ([CGPoint(x: 554, y: 420), CGPoint(x: 574, y: 250), CGPoint(x: 664, y: 150), CGPoint(x: 694, y: 60)], 0.14, -1),
]
for (c, curl, dir) in front { drawTentacle(spine(c, curl: curl, curlDir: dir), base: 128, dark: false) }

// ── The mantle: a dome, lit from below like a torch under the chin ───────
let mantle = CGMutablePath()
mantle.move(to: CGPoint(x: 262, y: 470))
mantle.addCurve(to: CGPoint(x: 512, y: 950), control1: CGPoint(x: 150, y: 680), control2: CGPoint(x: 250, y: 950))
mantle.addCurve(to: CGPoint(x: 762, y: 470), control1: CGPoint(x: 774, y: 950), control2: CGPoint(x: 874, y: 680))
// A scalloped underside where the arms begin.
mantle.addCurve(to: CGPoint(x: 512, y: 400), control1: CGPoint(x: 700, y: 400), control2: CGPoint(x: 600, y: 420))
mantle.addCurve(to: CGPoint(x: 252, y: 470), control1: CGPoint(x: 424, y: 420), control2: CGPoint(x: 324, y: 400))
mantle.closeSubpath()
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 30, color: rgb(0x000000, 0.7))
ctx.addPath(mantle); ctx.setFillColor(rgb(bruiseDark)); ctx.fillPath()
ctx.restoreGState()
clipped(mantle) {
    // Lit only from below: a dim violet jaw, the crown lost in shadow.
    ctx.drawLinearGradient(gradient([(0x5E44CC, 0, 1), (0x33227E, 0.3, 1), (0x180E40, 0.62, 1), (0x0A0620, 1, 1)]),
                           start: CGPoint(x: 0, y: 400), end: CGPoint(x: 0, y: 950), options: [])
    // The eye's light spilling onto the skin around it.
    ctx.drawRadialGradient(gradient([(blood, 0, 0.32), (blood, 1, 0)]), startCenter: CGPoint(x: 512, y: 610), startRadius: 120,
                           endCenter: CGPoint(x: 512, y: 610), endRadius: 330, options: [])
    // Bioluminescent rim along the bottom edge.
    ctx.drawRadialGradient(gradient([(biolume, 0, 0.55), (biolume, 1, 0)]), startCenter: CGPoint(x: 512, y: 330), startRadius: 0,
                           endCenter: CGPoint(x: 512, y: 330), endRadius: 260, options: [])
    // Mottled skin: a few soft darker spots high on the dome.
    for (x, y, r) in [(390.0, 820.0, 34.0), (640.0, 840.0, 26.0), (700.0, 760.0, 18.0), (330.0, 720.0, 20.0), (560.0, 880.0, 16.0)] {
        ctx.setFillColor(rgb(0x0E0826, 0.35))
        ctx.fillEllipse(in: CGRect(x: x - r, y: y - r, width: r * 2, height: r * 2))
    }
}

// ── The eye ──────────────────────────────────────────────────────────────
let eyeC = CGPoint(x: 512, y: 610)
let (ew, eh): (CGFloat, CGFloat) = (420, 236)
func almond(_ c: CGPoint, _ w: CGFloat, _ h: CGFloat) -> CGPath {
    let p = CGMutablePath()
    p.move(to: CGPoint(x: c.x - w / 2, y: c.y))
    p.addCurve(to: CGPoint(x: c.x + w / 2, y: c.y), control1: CGPoint(x: c.x - w * 0.22, y: c.y + h * 0.66), control2: CGPoint(x: c.x + w * 0.22, y: c.y + h * 0.66))
    p.addCurve(to: CGPoint(x: c.x - w / 2, y: c.y), control1: CGPoint(x: c.x + w * 0.22, y: c.y - h * 0.62), control2: CGPoint(x: c.x - w * 0.22, y: c.y - h * 0.62))
    p.closeSubpath()
    return p
}
// The socket: a dark, deep surround.
let socket = almond(eyeC, ew + 70, eh + 70)
ctx.addPath(socket); ctx.setFillColor(rgb(0x0A0518)); ctx.fillPath()
// A hot glow leaking from the eye into the socket.
clipped(socket) {
    ctx.drawRadialGradient(gradient([(blood, 0, 0.55), (blood, 1, 0)]), startCenter: eyeC, startRadius: 60, endCenter: eyeC, endRadius: 280, options: [])
}
let eye = almond(eyeC, ew, eh)
clipped(eye) {
    // Molten iris: amber core → blood → ember at the rim.
    ctx.drawRadialGradient(gradient([(amber, 0, 1), (0xFF8A2A, 0.28, 1), (blood, 0.58, 1), (ember, 0.86, 1), (0x3A0505, 1, 1)]),
                           startCenter: eyeC, startRadius: 0, endCenter: eyeC, endRadius: ew * 0.56, options: [.drawsAfterEndLocation])
    // Fine radial striations, like a real iris.
    ctx.setStrokeColor(rgb(0x5A0A04, 0.35)); ctx.setLineWidth(3)
    for i in 0..<56 {
        let a = CGFloat(i) / 56 * 2 * .pi
        ctx.move(to: CGPoint(x: eyeC.x + cos(a) * 70, y: eyeC.y + sin(a) * 70))
        ctx.addLine(to: CGPoint(x: eyeC.x + cos(a) * 250, y: eyeC.y + sin(a) * 250))
    }
    ctx.strokePath()
    // A heavy upper lid shadow: it looks *down* at you.
    ctx.drawLinearGradient(gradient([(0x000000, 0, 0), (0x000000, 1, 0.55)]),
                           start: CGPoint(x: 0, y: eyeC.y + 30), end: CGPoint(x: 0, y: eyeC.y + eh / 2), options: [.drawsAfterEndLocation])
}
// A heavy brow ridge in a V: it glares down at you. Its underside catches
// the eye's glow.
let brow = CGMutablePath()
brow.move(to: CGPoint(x: 248, y: 730))
brow.addCurve(to: CGPoint(x: 512, y: 650), control1: CGPoint(x: 350, y: 770), control2: CGPoint(x: 450, y: 712))
brow.addCurve(to: CGPoint(x: 776, y: 730), control1: CGPoint(x: 574, y: 712), control2: CGPoint(x: 674, y: 770))
brow.addCurve(to: CGPoint(x: 512, y: 790), control1: CGPoint(x: 700, y: 840), control2: CGPoint(x: 600, y: 830))
brow.addCurve(to: CGPoint(x: 248, y: 730), control1: CGPoint(x: 424, y: 830), control2: CGPoint(x: 324, y: 840))
brow.closeSubpath()
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -8), blur: 16, color: rgb(0x000000, 0.8))
ctx.addPath(brow); ctx.setFillColor(rgb(0x140B36)); ctx.fillPath()
ctx.restoreGState()
clipped(brow) {
    ctx.drawLinearGradient(gradient([(0xFF5A2A, 0, 0.6), (0x2A1660, 0.3, 1), (0x0C0624, 1, 1)]),
                           start: CGPoint(x: 0, y: 650), end: CGPoint(x: 0, y: 830), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
}

// The pupil: a terminal block cursor.
let pupil = CGRect(x: eyeC.x - 30, y: eyeC.y - 96, width: 60, height: 192)
ctx.saveGState()
ctx.setShadow(offset: .zero, blur: 14, color: rgb(0x000000, 0.9))
ctx.addPath(CGPath(roundedRect: pupil, cornerWidth: 10, cornerHeight: 10, transform: nil))
ctx.setFillColor(rgb(0x000000)); ctx.fillPath()
ctx.restoreGState()
// One small catchlight, so it feels alive.
ctx.setFillColor(rgb(0xFFF4E0, 0.85))
ctx.fillEllipse(in: CGRect(x: eyeC.x + 58, y: eyeC.y + 40, width: 26, height: 18))

NSGraphicsContext.restoreGraphicsState()
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon.png"
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out) (\(Int(px))px)")
