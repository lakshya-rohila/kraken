//
// KRAKEN — CINEMATIC PRODUCT FILM (WIDESCREEN 16:9 1080p @ 60 FPS)
// Perfect for YouTube, Desktop Web, Product Hunt, and X/Twitter
//
import Foundation
import CoreGraphics
import AppKit

let WIDTH: CGFloat = 1920.0
let HEIGHT: CGFloat = 1080.0
let INT_WIDTH: Int = 1920
let INT_HEIGHT: Int = 1080
let FPS: Double = 60.0
let TOTAL_FRAMES = 2520 // 42.0 seconds

print("Compiling Kraken Widescreen (16:9 1080p) Product Film Renderer...")
print("Target: 1920x1080 @ 60fps | 42.0s (\(TOTAL_FRAMES) frames)")

let bytesPerRow = INT_WIDTH * 4
let totalBytes = bytesPerRow * INT_HEIGHT
let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: totalBytes)
defer { buffer.deallocate() }
let colorSpace = CGColorSpaceCreateDeviceRGB()

guard let ctx = CGContext(
    data: buffer,
    width: INT_WIDTH,
    height: INT_HEIGHT,
    bitsPerComponent: 8,
    bytesPerRow: bytesPerRow,
    space: colorSpace,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else {
    print("Failed to create CGContext")
    exit(1)
}

let iconImage = NSImage(contentsOfFile: "assets/icon-1024.png")

// Colors
func rgb(_ hex: UInt32, _ alpha: CGFloat = 1.0) -> CGColor {
    let r = CGFloat((hex >> 16) & 0xff) / 255.0
    let g = CGFloat((hex >> 8) & 0xff) / 255.0
    let b = CGFloat(hex & 0xff) / 255.0
    return CGColor(colorSpace: colorSpace, components: [r, g, b, alpha])!
}

let cWindowBg = rgb(0x0A0E17, 0.94)
let cTitleBar = rgb(0x101522, 0.96)
let cGlassBorder = rgb(0xFFFFFF, 0.12)
let cGlassHighlight = rgb(0xFFFFFF, 0.22)
let cWhite = rgb(0xFFFFFF)
let cTextPrimary = rgb(0xF0F4FC)
let cTextSecondary = rgb(0x8B98AD)
let cTextMuted = rgb(0x505C72)
let cCyan = rgb(0x38E1CA)
let cCyanSoft = rgb(0x38E1CA, 0.25)
let cBlue = rgb(0x3E98F5)
let cPurple = rgb(0x8C7BFF)
let cAmber = rgb(0xF2B94B)
let cGreen = rgb(0x38D996)

func clamp<T: Comparable>(_ val: T, _ minV: T, _ maxV: T) -> T {
    return min(max(val, minV), maxV)
}

func lerp(_ a: CGFloat, _ b: CGFloat, _ t: CGFloat) -> CGFloat {
    return a + (b - a) * t
}

func easeOutCubic(_ t: CGFloat) -> CGFloat {
    let t1 = t - 1.0
    return t1 * t1 * t1 + 1.0
}

func easeInOutCubic(_ t: CGFloat) -> CGFloat {
    return t < 0.5 ? 4.0 * t * t * t : 1.0 - pow(-2.0 * t + 2.0, 3.0) / 2.0
}

func easeOutBack(_ t: CGFloat, overshoot: CGFloat = 1.4) -> CGFloat {
    let c1: CGFloat = overshoot
    let c3 = c1 + 1.0
    let t1 = t - 1.0
    return 1.0 + c3 * t1 * t1 * t1 + c1 * t1 * t1
}

func drawText(
    _ text: String,
    x: CGFloat,
    y: CGFloat,
    fontSize: CGFloat,
    weight: NSFont.Weight = .regular,
    mono: Bool = false,
    color: CGColor,
    align: NSTextAlignment = .center,
    letterSpacing: CGFloat = 0.0
) {
    let font: NSFont = mono ? (NSFont(name: "Menlo", size: fontSize) ?? NSFont.monospacedSystemFont(ofSize: fontSize, weight: weight)) : NSFont.systemFont(ofSize: fontSize, weight: weight)
    let paragraphStyle = NSMutableParagraphStyle()
    paragraphStyle.alignment = align
    
    var attrs: [NSAttributedString.Key: Any] = [
        .font: font,
        .foregroundColor: NSColor(cgColor: color) ?? .white,
        .paragraphStyle: paragraphStyle
    ]
    if letterSpacing != 0.0 {
        attrs[.kern] = letterSpacing
    }
    
    let attrString = NSAttributedString(string: text, attributes: attrs)
    let size = attrString.size()
    
    var drawX = x
    if align == .center {
        drawX = x - size.width / 2.0
    } else if align == .right {
        drawX = x - size.width
    }
    let drawY = y - size.height / 2.0
    
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
    attrString.draw(at: NSPoint(x: drawX, y: drawY))
    NSGraphicsContext.restoreGraphicsState()
}

func drawGlassRect(
    _ rect: CGRect,
    radius: CGFloat,
    fill: CGColor = cWindowBg,
    borderColor: CGColor = cGlassBorder,
    borderWidth: CGFloat = 1.0,
    shadowBlur: CGFloat = 30.0,
    shadowColor: CGColor = rgb(0x000000, 0.6)
) {
    ctx.saveGState()
    if shadowBlur > 0.0 {
        ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: shadowBlur, color: shadowColor)
    }
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    ctx.addPath(path)
    ctx.setFillColor(fill)
    ctx.fillPath()
    
    ctx.addPath(path)
    ctx.setStrokeColor(borderColor)
    ctx.setLineWidth(borderWidth)
    ctx.strokePath()
    ctx.restoreGState()
}

func drawTrafficLights(origin: CGPoint) {
    let colors = [rgb(0xFF5F56), rgb(0xFFBD2E), rgb(0x27C93F)]
    for (i, c) in colors.enumerated() {
        let circleRect = CGRect(x: origin.x + CGFloat(i * 22), y: origin.y, width: 13, height: 13)
        ctx.setFillColor(c)
        ctx.fillEllipse(in: circleRect)
        ctx.setStrokeColor(rgb(0x000000, 0.2))
        ctx.setLineWidth(0.5)
        ctx.strokeEllipse(in: circleRect)
    }
}

func drawMacCursor(at point: CGPoint, isClicked: Bool = false) {
    ctx.saveGState()
    ctx.translateBy(x: point.x, y: point.y)
    let s: CGFloat = isClicked ? 0.88 : 1.0
    ctx.scaleBy(x: s, y: s)
    
    ctx.setShadow(offset: CGSize(width: 2, height: -3), blur: 6, color: rgb(0x000000, 0.45))
    
    let path = CGMutablePath()
    path.move(to: CGPoint(x: 0, y: 0))
    path.addLine(to: CGPoint(x: 0, y: -22))
    path.addLine(to: CGPoint(x: 5.5, y: -16.5))
    path.addLine(to: CGPoint(x: 9.5, y: -25.5))
    path.addLine(to: CGPoint(x: 13.5, y: -23.5))
    path.addLine(to: CGPoint(x: 9.5, y: -14.5))
    path.addLine(to: CGPoint(x: 16.0, y: -14.5))
    path.closeSubpath()
    
    ctx.addPath(path)
    ctx.setFillColor(rgb(0x050505))
    ctx.fillPath()
    
    ctx.addPath(path)
    ctx.setStrokeColor(cWhite)
    ctx.setLineWidth(1.5)
    ctx.strokePath()
    ctx.restoreGState()
}

func drawMacDesktop(pulse: CGFloat = 0.0) {
    let grad = CGGradient(
        colorsSpace: colorSpace,
        colors: [rgb(0x0E1729), rgb(0x080B11), rgb(0x040609)] as CFArray,
        locations: [1.0, 0.45, 0.0]
    )!
    ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: HEIGHT), end: CGPoint(x: 0, y: 0), options: [])
    
    ctx.saveGState()
    let glowGrad = CGGradient(
        colorsSpace: colorSpace,
        colors: [rgb(0x1A2B4C, 0.40 + pulse * 0.15), rgb(0x000000, 0.0)] as CFArray,
        locations: [0.0, 1.0]
    )!
    ctx.drawRadialGradient(
        glowGrad,
        startCenter: CGPoint(x: WIDTH / 2, y: HEIGHT / 2),
        startRadius: 50,
        endCenter: CGPoint(x: WIDTH / 2, y: HEIGHT / 2),
        endRadius: 900,
        options: []
    )
    ctx.restoreGState()
    
    // Top macOS Menu Bar
    let menuH: CGFloat = 38
    let menuY = HEIGHT - menuH
    ctx.setFillColor(rgb(0x0A0E17, 0.70))
    ctx.fill(CGRect(x: 0, y: menuY, width: WIDTH, height: menuH))
    
    drawText("", x: 30, y: menuY + menuH / 2, fontSize: 16, weight: .bold, color: cTextPrimary, align: .center)
    drawText("Kraken", x: 65, y: menuY + menuH / 2, fontSize: 14, weight: .heavy, color: cTextPrimary, align: .left)
    drawText("File", x: 135, y: menuY + menuH / 2, fontSize: 13, weight: .regular, color: cTextSecondary, align: .left)
    drawText("Edit", x: 175, y: menuY + menuH / 2, fontSize: 13, weight: .regular, color: cTextSecondary, align: .left)
    drawText("View", x: 215, y: menuY + menuH / 2, fontSize: 13, weight: .regular, color: cTextSecondary, align: .left)
    drawText("Window", x: 260, y: menuY + menuH / 2, fontSize: 13, weight: .regular, color: cTextSecondary, align: .left)
    drawText("Help", x: 325, y: menuY + menuH / 2, fontSize: 13, weight: .regular, color: cTextSecondary, align: .left)
    
    drawText("9:41 AM", x: WIDTH - 35, y: menuY + menuH / 2, fontSize: 13, weight: .medium, color: cTextPrimary, align: .right)
    drawText("Tue Nov 9", x: WIDTH - 110, y: menuY + menuH / 2, fontSize: 13, weight: .regular, color: cTextSecondary, align: .right)
}

// ----------------- ACT 1: LAUNCH (0.0s – 4.6s) ----------------- //
func renderAct1(frame: Int) {
    drawMacDesktop()
    let subF = frame
    let spotAlpha = clamp(CGFloat(subF - 30) / 20.0, 0.0, 1.0)
    
    if spotAlpha > 0.0 && subF < 260 {
        let spotW: CGFloat = 720
        let spotH: CGFloat = subF > 130 ? 210 : 72
        let spotX = (WIDTH - spotW) / 2
        let spotY = HEIGHT / 2 + 80
        
        let spotRect = CGRect(x: spotX, y: spotY - (spotH - 72), width: spotW, height: spotH)
        drawGlassRect(spotRect, radius: 18, fill: rgb(0x131926, 0.90), borderColor: cGlassHighlight, borderWidth: 1.0, shadowBlur: 40)
        
        ctx.saveGState()
        ctx.setStrokeColor(cCyan)
        ctx.setLineWidth(2.2)
        ctx.strokeEllipse(in: CGRect(x: spotX + 26, y: spotY + 28, width: 14, height: 14))
        ctx.move(to: CGPoint(x: spotX + 37, y: spotY + 31))
        ctx.addLine(to: CGPoint(x: spotX + 44, y: spotY + 24))
        ctx.strokePath()
        ctx.restoreGState()
        
        let fullQuery = "Kraken"
        let typeProgress = clamp(CGFloat(subF - 60) / 60.0, 0.0, 1.0)
        let charsCount = Int(typeProgress * CGFloat(fullQuery.count))
        let typedText = String(fullQuery.prefix(charsCount))
        
        drawText(typedText, x: spotX + 68, y: spotY + 36, fontSize: 26, weight: .medium, color: cWhite, align: .left)
        
        if (subF / 20) % 2 == 0 && subF < 160 {
            let cursorX = spotX + 70 + CGFloat(charsCount) * 16.0
            ctx.setFillColor(cCyan)
            ctx.fill(CGRect(x: cursorX, y: spotY + 18, width: 2.5, height: 32))
        }
        
        if subF > 130 {
            let resY = spotY - 60
            let resRect = CGRect(x: spotX + 12, y: resY - 24, width: spotW - 24, height: 60)
            drawGlassRect(resRect, radius: 12, fill: rgb(0x233148, 0.95), borderColor: rgb(0x3E98F5, 0.5), borderWidth: 1.0, shadowBlur: 0)
            
            if let icon = iconImage {
                let imgRect = CGRect(x: spotX + 26, y: resY - 14, width: 40, height: 40)
                NSGraphicsContext.saveGraphicsState()
                NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
                icon.draw(in: imgRect)
                NSGraphicsContext.restoreGraphicsState()
            }
            
            drawText("Kraken.app", x: spotX + 80, y: resY + 12, fontSize: 18, weight: .bold, color: cWhite, align: .left)
            drawText("GPU-Accelerated Terminal — Application", x: spotX + 80, y: resY - 8, fontSize: 14, weight: .regular, color: cTextSecondary, align: .left)
            drawText("↩ Enter", x: spotX + spotW - 35, y: resY + 4, fontSize: 14, weight: .bold, color: cTextMuted, align: .right)
        }
    }
    
    if subF >= 230 && subF < 276 {
        let launchProgress = CGFloat(subF - 230) / 46.0
        let scale = lerp(0.4, 1.0, easeOutBack(launchProgress, overshoot: 1.1))
        let alpha = min(1.0, launchProgress * 2.0)
        
        ctx.saveGState()
        ctx.translateBy(x: WIDTH / 2, y: HEIGHT / 2 - 20)
        ctx.scaleBy(x: scale, y: scale)
        ctx.setAlpha(alpha)
        
        let winW: CGFloat = 1360
        let winH: CGFloat = 820
        drawGlassRect(CGRect(x: -winW / 2, y: -winH / 2, width: winW, height: winH), radius: 20, fill: cWindowBg, borderColor: cGlassHighlight, borderWidth: 1.5, shadowBlur: 50)
        ctx.restoreGState()
    }
}

// ----------------- ACT 2: SPEED (4.6s – 12.0s) ----------------- //
func renderAct2(frame: Int) {
    let subF = frame - 277
    let progress = CGFloat(subF) / CGFloat(720 - 277)
    let pulse = subF < 30 ? (1.0 - CGFloat(subF) / 30.0) : 0.0
    drawMacDesktop(pulse: pulse)
    
    let camZoom = lerp(1.0, 1.03, easeInOutCubic(progress))
    ctx.saveGState()
    ctx.translateBy(x: WIDTH / 2, y: HEIGHT / 2 - 20)
    ctx.scaleBy(x: camZoom, y: camZoom)
    
    let winW: CGFloat = 1380
    let winH: CGFloat = 820
    let winX = -winW / 2
    let winY = -winH / 2
    drawGlassRect(CGRect(x: winX, y: winY, width: winW, height: winH), radius: 20, fill: cWindowBg, borderColor: cGlassHighlight, borderWidth: 1.2, shadowBlur: 60)
    
    let titleH: CGFloat = 48
    drawGlassRect(CGRect(x: winX, y: winY + winH - titleH, width: winW, height: titleH), radius: 20, fill: cTitleBar, borderColor: rgb(0, 0), borderWidth: 0, shadowBlur: 0)
    ctx.setFillColor(cTitleBar)
    ctx.fill(CGRect(x: winX, y: winY + winH - titleH, width: winW, height: 20))
    drawTrafficLights(origin: CGPoint(x: winX + 20, y: winY + winH - 30))
    
    let tabW: CGFloat = 280
    drawGlassRect(CGRect(x: winX + 105, y: winY + winH - titleH + 6, width: tabW, height: titleH - 6), radius: 8, fill: cWindowBg, borderColor: rgb(0xFFFFFF, 0.06), borderWidth: 1.0, shadowBlur: 0)
    drawText("kraken — ~/Developer/Kraken", x: winX + 105 + tabW / 2, y: winY + winH - 24, fontSize: 13, weight: .medium, color: cTextPrimary, align: .center)
    ctx.setFillColor(cCyan)
    ctx.fill(CGRect(x: winX + 105, y: winY + winH - titleH + 6, width: tabW, height: 2.5))
    
    let termX = winX + 36
    var ty = winY + winH - 90
    drawText("kraken 🌿 main ➜ git status", x: termX, y: ty, fontSize: 20, weight: .bold, mono: true, color: cCyan, align: .left)
    ty -= 34
    drawText("On branch main — working tree clean", x: termX, y: ty, fontSize: 18, weight: .regular, mono: true, color: cTextSecondary, align: .left)
    ty -= 38
    
    drawText("kraken 🌿 main ➜ cargo build --release", x: termX, y: ty, fontSize: 20, weight: .bold, mono: true, color: cCyan, align: .left)
    ty -= 34
    
    let cargoLines = [
        ("   Compiling kraken-render v0.1.0 (Metal 3 GPU Pipeline)", cTextSecondary),
        ("   Compiling kraken-mux v0.1.0", cTextSecondary),
        ("   Compiling kraken-app v0.1.0", cTextSecondary),
        ("    Finished release [optimized] in 0.84s", cGreen)
    ]
    let linesToShow = min(cargoLines.count, Int(progress * 8.0))
    for i in 0..<linesToShow {
        let (txt, col) = cargoLines[i]
        drawText(txt, x: termX, y: ty, fontSize: 18, weight: i == cargoLines.count - 1 ? .bold : .regular, mono: true, color: col, align: .left)
        ty -= 30
    }
    
    let statusH: CGFloat = 46
    drawGlassRect(CGRect(x: winX, y: winY, width: winW, height: statusH), radius: 20, fill: cTitleBar, borderColor: rgb(0, 0), borderWidth: 0, shadowBlur: 0)
    ctx.setFillColor(cTitleBar)
    ctx.fill(CGRect(x: winX, y: winY + statusH - 20, width: winW, height: 20))
    drawText("📁 Kraken", x: winX + 24, y: winY + 23, fontSize: 14, weight: .bold, color: cCyan, align: .left)
    drawText("🌿 main (+0 ~0)", x: winX + 130, y: winY + 23, fontSize: 14, weight: .medium, color: cGreen, align: .left)
    drawText("⏱ 0.84s ✔", x: winX + 270, y: winY + 23, fontSize: 14, weight: .medium, color: cAmber, align: .left)
    drawText("120Hz PROMOTION • METAL 3", x: winX + winW - 24, y: winY + 23, fontSize: 13, weight: .heavy, color: cTextSecondary, align: .right, letterSpacing: 1.5)
    
    ctx.restoreGState()
    
    let badgeAlpha = clamp(CGFloat(subF - 40) / 30.0, 0.0, 1.0)
    if badgeAlpha > 0.0 {
        ctx.saveGState()
        ctx.setAlpha(badgeAlpha)
        let pillH: CGFloat = 46
        let pillW: CGFloat = 680
        let pillX = (WIDTH - pillW) / 2
        let pillY: CGFloat = 42
        drawGlassRect(CGRect(x: pillX, y: pillY, width: pillW, height: pillH), radius: pillH / 2, fill: rgb(0x101624, 0.92), borderColor: cGlassHighlight, borderWidth: 1.0, shadowBlur: 30)
        drawText("⚡ METAL 3 ENGINE", x: pillX + 110, y: pillY + pillH / 2, fontSize: 15, weight: .heavy, color: cCyan, align: .center, letterSpacing: 1.5)
        drawText("•", x: pillX + 225, y: pillY + pillH / 2, fontSize: 15, weight: .regular, color: cTextMuted, align: .center)
        drawText("🚀 120Hz PROMOTION", x: pillX + 340, y: pillY + pillH / 2, fontSize: 15, weight: .heavy, color: cBlue, align: .center, letterSpacing: 1.5)
        drawText("•", x: pillX + 455, y: pillY + pillH / 2, fontSize: 15, weight: .regular, color: cTextMuted, align: .center)
        drawText("< 4.8ms LATENCY", x: pillX + 570, y: pillY + pillH / 2, fontSize: 15, weight: .heavy, color: cGreen, align: .center, letterSpacing: 1.5)
        ctx.restoreGState()
    }
}

// ----------------- ACT 3: CODING AGENTS (12.0s – 22.0s) ----------------- //
func renderAct3(frame: Int) {
    let subF = frame - 721
    drawMacDesktop()
    
    drawText("BUILT FOR THE AGENT ERA", x: WIDTH / 2, y: HEIGHT - 68, fontSize: 15, weight: .heavy, color: cAmber, align: .center, letterSpacing: 3)
    drawText("Answer Coding Agents Without Leaving Your Tab", x: WIDTH / 2, y: HEIGHT - 102, fontSize: 30, weight: .bold, color: cWhite, align: .center)
    
    let winW: CGFloat = 1380
    let winH: CGFloat = 820
    let winX = (WIDTH - winW) / 2
    let winY: CGFloat = 100
    drawGlassRect(CGRect(x: winX, y: winY, width: winW, height: winH), radius: 20, fill: cWindowBg, borderColor: cGlassHighlight, borderWidth: 1.2, shadowBlur: 60)
    
    let titleH: CGFloat = 48
    drawGlassRect(CGRect(x: winX, y: winY + winH - titleH, width: winW, height: titleH), radius: 20, fill: cTitleBar, borderColor: rgb(0, 0), borderWidth: 0, shadowBlur: 0)
    ctx.setFillColor(cTitleBar)
    ctx.fill(CGRect(x: winX, y: winY + winH - titleH, width: winW, height: 20))
    drawTrafficLights(origin: CGPoint(x: winX + 20, y: winY + winH - 30))
    drawText("Tab 2 • Claude Code (Running)", x: winX + 180, y: winY + winH - 24, fontSize: 13, weight: .bold, color: cAmber, align: .left)
    
    let termX = winX + 36
    var ty = winY + winH - 90
    drawText("claude ➜ analyzing project architecture...", x: termX, y: ty, fontSize: 20, weight: .bold, mono: true, color: cCyan, align: .left)
    ty -= 34
    drawText("✔ Analyzed 6 crates in workspace", x: termX, y: ty, fontSize: 18, weight: .regular, mono: true, color: cTextSecondary, align: .left)
    ty -= 30
    drawText("✔ Verified zero compiler warnings in kraken-render", x: termX, y: ty, fontSize: 18, weight: .regular, mono: true, color: cTextSecondary, align: .left)
    ty -= 30
    drawText("✔ Ready to apply GPU instancing optimization", x: termX, y: ty, fontSize: 18, weight: .regular, mono: true, color: cGreen, align: .left)
    ty -= 44
    drawText("Do you want to make this edit to main.rs?", x: termX, y: ty, fontSize: 20, weight: .bold, mono: true, color: cAmber, align: .left)
    ty -= 30
    drawText("  1. Yes", x: termX, y: ty, fontSize: 18, weight: .bold, mono: true, color: cWhite, align: .left)
    ty -= 26
    drawText("  2. Always allow during this session", x: termX, y: ty, fontSize: 18, weight: .regular, mono: true, color: cTextMuted, align: .left)
    ty -= 26
    drawText("  3. No", x: termX, y: ty, fontSize: 18, weight: .regular, mono: true, color: cTextMuted, align: .left)
    
    // Floating "Needs You" Card (Widescreen layout: slides from right side)
    let slideInT = clamp(CGFloat(subF - 40) / 45.0, 0.0, 1.0)
    let cardEased = easeOutBack(slideInT, overshoot: 1.15)
    
    let cardW: CGFloat = 580
    let cardH: CGFloat = 260
    let cardTargetX = winX + winW - cardW - 30
    let cardStartX = WIDTH + 40
    let cardCurrentX = lerp(cardStartX, cardTargetX, cardEased)
    let cardY = winY + 60
    
    let cardRect = CGRect(x: cardCurrentX, y: cardY, width: cardW, height: cardH)
    drawGlassRect(cardRect, radius: 20, fill: rgb(0x131A29, 0.96), borderColor: cAmber, borderWidth: 1.5, shadowBlur: 45, shadowColor: rgb(0xF2B94B, 0.25))
    
    drawText("●", x: cardCurrentX + 30, y: cardY + cardH - 38, fontSize: 14, weight: .bold, color: cAmber, align: .center)
    drawText("CLAUDE CODE NEEDS YOU", x: cardCurrentX + 48, y: cardY + cardH - 38, fontSize: 14, weight: .black, color: cAmber, align: .left, letterSpacing: 2)
    drawText("Tab 2", x: cardCurrentX + cardW - 30, y: cardY + cardH - 38, fontSize: 13, weight: .medium, color: cTextMuted, align: .right)
    
    drawText("Do you want to make this edit to main.rs?", x: cardCurrentX + 30, y: cardY + cardH - 85, fontSize: 20, weight: .bold, color: cWhite, align: .left)
    drawText("Apply GPU instancing optimization to render loop", x: cardCurrentX + 30, y: cardY + cardH - 115, fontSize: 15, weight: .regular, color: cTextSecondary, align: .left)
    
    let btnY = cardY + 30
    let btnH: CGFloat = 50
    let btn1W: CGFloat = 150
    let btn2W: CGFloat = 180
    let btn3W: CGFloat = 130
    
    let isClicked = (subF > 320)
    let b1Bg = isClicked ? cGreen : rgb(0x1D283C, 0.9)
    let b1Border = isClicked ? cWhite : rgb(0x38D996, 0.6)
    let b1TextCol = isClicked ? rgb(0x06111A) : cGreen
    
    let b1Rect = CGRect(x: cardCurrentX + 30, y: btnY, width: btn1W, height: btnH)
    drawGlassRect(b1Rect, radius: 12, fill: b1Bg, borderColor: b1Border, borderWidth: 1.5, shadowBlur: isClicked ? 25 : 0, shadowColor: cGreen)
    drawText("1  Yes", x: b1Rect.midX, y: b1Rect.midY, fontSize: 18, weight: .bold, color: b1TextCol, align: .center)
    
    let b2Rect = CGRect(x: cardCurrentX + 30 + btn1W + 15, y: btnY, width: btn2W, height: btnH)
    drawGlassRect(b2Rect, radius: 12, fill: rgb(0x151E2E, 0.8), borderColor: cGlassBorder, borderWidth: 1.0, shadowBlur: 0)
    drawText("2  Always", x: b2Rect.midX, y: b2Rect.midY, fontSize: 16, weight: .medium, color: cTextSecondary, align: .center)
    
    let b3Rect = CGRect(x: cardCurrentX + 30 + btn1W + btn2W + 30, y: btnY, width: btn3W, height: btnH)
    drawGlassRect(b3Rect, radius: 12, fill: rgb(0x151E2E, 0.8), borderColor: cGlassBorder, borderWidth: 1.0, shadowBlur: 0)
    drawText("3  No", x: b3Rect.midX, y: b3Rect.midY, fontSize: 16, weight: .medium, color: cTextMuted, align: .center)
    
    if subF > 180 && subF < 420 {
        let cursorT = clamp(CGFloat(subF - 180) / 130.0, 0.0, 1.0)
        let curEased = easeInOutCubic(cursorT)
        let startCur = CGPoint(x: WIDTH - 100, y: 150)
        let targetCur = CGPoint(x: b1Rect.midX + 15, y: b1Rect.midY - 8)
        let curPos = CGPoint(x: lerp(startCur.x, targetCur.x, curEased), y: lerp(startCur.y, targetCur.y, curEased))
        drawMacCursor(at: curPos, isClicked: (subF >= 315 && subF <= 335))
    }
}

// ----------------- ACT 4: SPLITS & MEDIA (22.0s – 32.0s) ----------------- //
func renderAct4(frame: Int) {
    drawMacDesktop()
    
    drawText("UNRIVALED WORKSPACE PRODUCTIVITY", x: WIDTH / 2, y: HEIGHT - 68, fontSize: 15, weight: .heavy, color: cCyan, align: .center, letterSpacing: 3)
    drawText("Splits • File Sidebar • Inline Media Protocol", x: WIDTH / 2, y: HEIGHT - 102, fontSize: 30, weight: .bold, color: cWhite, align: .center)
    
    let winW: CGFloat = 1420
    let winH: CGFloat = 820
    let winX = (WIDTH - winW) / 2
    let winY: CGFloat = 90
    drawGlassRect(CGRect(x: winX, y: winY, width: winW, height: winH), radius: 20, fill: cWindowBg, borderColor: cGlassHighlight, borderWidth: 1.2, shadowBlur: 60)
    
    let titleH: CGFloat = 48
    drawGlassRect(CGRect(x: winX, y: winY + winH - titleH, width: winW, height: titleH), radius: 20, fill: cTitleBar, borderColor: rgb(0, 0), borderWidth: 0, shadowBlur: 0)
    ctx.setFillColor(cTitleBar)
    ctx.fill(CGRect(x: winX, y: winY + winH - titleH, width: winW, height: 20))
    drawTrafficLights(origin: CGPoint(x: winX + 20, y: winY + winH - 30))
    
    let tabW: CGFloat = 260
    drawGlassRect(CGRect(x: winX + 105, y: winY + winH - titleH + 6, width: tabW, height: titleH - 6), radius: 8, fill: cWindowBg, borderColor: rgb(0xFFFFFF, 0.06), borderWidth: 1.0, shadowBlur: 0)
    drawText("kraken — 3 Splits", x: winX + 105 + tabW / 2, y: winY + winH - 24, fontSize: 13, weight: .medium, color: cTextPrimary, align: .center)
    ctx.setFillColor(cCyan)
    ctx.fill(CGRect(x: winX + 105, y: winY + winH - titleH + 6, width: tabW, height: 2.5))
    
    let contentY = winY
    let contentH = winH - titleH
    
    // Pane 1: Sidebar (w: 280)
    let sbW: CGFloat = 280
    ctx.setFillColor(rgb(0x0C101A, 0.95))
    ctx.fill(CGRect(x: winX, y: contentY, width: sbW, height: contentH))
    
    drawText("EXPLORER  ⌘B", x: winX + 24, y: winY + winH - 80, fontSize: 13, weight: .heavy, color: cTextMuted, align: .left, letterSpacing: 1.5)
    var sy = winY + winH - 115
    let files = [
        ("📁 crates", cCyan),
        ("  📁 kraken-render", cTextPrimary),
        ("    🦀 lib.rs", cGreen),
        ("  📁 kraken-ui", cTextPrimary),
        ("    🦀 cards.rs", cAmber),
        ("  📁 kraken-mux", cTextPrimary),
        ("    🦀 layout.rs", cGreen),
        ("⚙️ Cargo.toml", cTextSecondary),
        ("📄 PLAN.md", cTextMuted)
    ]
    for (fName, fCol) in files {
        drawText(fName, x: winX + 24, y: sy, fontSize: 16, weight: .medium, mono: true, color: fCol, align: .left)
        sy -= 30
    }
    
    ctx.setFillColor(cGlassBorder)
    ctx.fill(CGRect(x: winX + sbW, y: contentY, width: 1.5, height: contentH))
    
    // Pane 2: Editor (w: 580)
    let cx = winX + sbW + 1.5
    let centerW: CGFloat = 580
    drawText("src/lib.rs (nvim)", x: cx + 22, y: winY + winH - 80, fontSize: 15, weight: .bold, mono: true, color: cAmber, align: .left)
    
    var cy = winY + winH - 115
    let codeSnippet = [
        "pub fn render_frame() {",
        "    let mut frame = Frame::default();",
        "    frame.draw_grid();",
        "    frame.draw_images();",
        "    // 120Hz Metal Command Queue",
        "    queue.submit(encoder.finish());",
        "}"
    ]
    for cLine in codeSnippet {
        drawText(cLine, x: cx + 22, y: cy, fontSize: 16, weight: .regular, mono: true, color: cTextSecondary, align: .left)
        cy -= 28
    }
    
    ctx.setFillColor(cGlassBorder)
    ctx.fill(CGRect(x: cx + centerW, y: contentY, width: 1.5, height: contentH))
    
    // Pane 3: Inline Kitty Protocol (w: remainder)
    let rx = cx + centerW + 1.5
    let rightW = winW - sbW - centerW - 3
    drawText("🖼 KITTY GRAPHICS PROTOCOL", x: rx + 20, y: winY + winH - 80, fontSize: 14, weight: .heavy, color: cGreen, align: .left, letterSpacing: 1.5)
    
    if let icon = iconImage {
        let imgW: CGFloat = 260
        let imgRect = CGRect(x: rx + (rightW - imgW) / 2, y: winY + 220, width: imgW, height: imgW)
        drawGlassRect(imgRect, radius: 14, fill: rgb(0x121A28, 0.8), borderColor: cGlassBorder, borderWidth: 1.0, shadowBlur: 15)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
        icon.draw(in: imgRect)
        NSGraphicsContext.restoreGraphicsState()
    }
    
    drawText("Kraken.icns (2048x2048)", x: rx + rightW / 2, y: winY + 180, fontSize: 14, weight: .medium, mono: true, color: cTextSecondary, align: .center)
    drawText("Kitty Graphics • OSC 1337 • Sixel", x: rx + rightW / 2, y: winY + 145, fontSize: 13, weight: .bold, color: cGreen, align: .center)
}

// ----------------- ACT 5: THEMES & OUTRO (32.0s – 42.0s) ----------------- //
func renderAct5(frame: Int) {
    let subF = frame - 1921
    drawMacDesktop()
    
    if subF < 240 {
        let themeIdx = (subF / 60) % 4
        let themeTitles = ["KRAKEN ABYSS", "MIDNIGHT INK", "NEON REEF", "DEEP SEA"]
        let themeAccents = [cCyan, cTextPrimary, rgb(0xFF4FD8), rgb(0x2EE6A8)]
        let curTitle = themeTitles[themeIdx]
        let curAccent = themeAccents[themeIdx]
        
        drawText("BESPOKE ACCESSIBLE THEMES", x: WIDTH / 2, y: HEIGHT - 75, fontSize: 15, weight: .heavy, color: curAccent, align: .center, letterSpacing: 3)
        drawText(curTitle, x: WIDTH / 2, y: HEIGHT - 115, fontSize: 36, weight: .black, color: cWhite, align: .center)
        
        let winW: CGFloat = 1100
        let winH: CGFloat = 720
        let winX = (WIDTH - winW) / 2
        let winY: CGFloat = 130
        drawGlassRect(CGRect(x: winX, y: winY, width: winW, height: winH), radius: 20, fill: cWindowBg, borderColor: curAccent, borderWidth: 1.5, shadowBlur: 40, shadowColor: curAccent)
        
        let sway = sin(Double(frame) * 0.12) * 5.0
        var ty = winY + winH - 100
        let asciiLines = [
            "                 .---.                ",
            "                /     \\               ",
            "               | () () |   < KRAKEN > ",
            "                \\  -  /               ",
            "            /\\   `---'   /\\           ",
            "           /  \\         /  \\          ",
            "          ~    ~       ~    ~         ",
            "         (  GPU-ACCELERATED  )        ",
            "          `~~~~~~~~~~~~~~~~~'         "
        ]
        for line in asciiLines {
            drawText(line, x: WIDTH / 2 + CGFloat(sway), y: ty, fontSize: 22, weight: .bold, mono: true, color: curAccent, align: .center)
            ty -= 30
        }
        drawText("WCAG AAA Compliance • Native macOS Vibrancy", x: WIDTH / 2, y: winY + 60, fontSize: 16, weight: .medium, color: cTextSecondary, align: .center)
    } else {
        let outroF = subF - 240
        let outroProgress = CGFloat(outroF) / CGFloat(2520 - 2161)
        let fadeOut = outroProgress > 0.85 ? (1.0 - (outroProgress - 0.85) / 0.15) : 1.0
        
        ctx.saveGState()
        ctx.setAlpha(fadeOut)
        
        if let icon = iconImage {
            let iconSize: CGFloat = 260
            let iconRect = CGRect(x: (WIDTH - iconSize) / 2, y: HEIGHT / 2 + 80, width: iconSize, height: iconSize)
            ctx.saveGState()
            ctx.setShadow(offset: .zero, blur: 60, color: cCyanSoft)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
            icon.draw(in: iconRect)
            NSGraphicsContext.restoreGraphicsState()
            ctx.restoreGState()
        }
        
        drawText("KRAKEN", x: WIDTH / 2, y: HEIGHT / 2 + 10, fontSize: 82, weight: .black, color: cWhite, align: .center, letterSpacing: 14)
        drawText("The Native GPU Terminal for the Agent Era", x: WIDTH / 2, y: HEIGHT / 2 - 50, fontSize: 22, weight: .medium, color: cCyan, align: .center, letterSpacing: 3)
        
        let ctaH: CGFloat = 58
        let ctaW: CGFloat = 440
        let ctaX = (WIDTH - ctaW) / 2
        let ctaY: CGFloat = HEIGHT / 2 - 160
        drawGlassRect(CGRect(x: ctaX, y: ctaY, width: ctaW, height: ctaH), radius: ctaH / 2, fill: cCyan, borderColor: cWhite, borderWidth: 1.0, shadowBlur: 35, shadowColor: cCyanSoft)
        drawText("UNLEASH THE KRAKEN", x: ctaX + ctaW / 2, y: ctaY + ctaH / 2, fontSize: 18, weight: .black, color: rgb(0x06111A), align: .center, letterSpacing: 3)
        
        drawText("github.com/kraken-term/kraken", x: WIDTH / 2, y: ctaY - 50, fontSize: 16, weight: .medium, mono: true, color: cTextMuted, align: .center)
        
        ctx.restoreGState()
    }
}

// Main Render Loop
let outputHandle = FileHandle.standardOutput

for f in 0..<TOTAL_FRAMES {
    ctx.clear(CGRect(x: 0, y: 0, width: WIDTH, height: HEIGHT))
    if f < 277 {
        renderAct1(frame: f)
    } else if f < 721 {
        renderAct2(frame: f)
    } else if f < 1321 {
        renderAct3(frame: f)
    } else if f < 1921 {
        renderAct4(frame: f)
    } else {
        renderAct5(frame: f)
    }
    
    let chunk = Data(bytesNoCopy: buffer, count: totalBytes, deallocator: .none)
    outputHandle.write(chunk)
    
    if (f + 1) % 120 == 0 || f == TOTAL_FRAMES - 1 {
        let percent = Double(f + 1) / Double(TOTAL_FRAMES) * 100.0
        fputs(String(format: "\r[Widescreen Progress] %d / %d frames (%.1f%%)", f + 1, TOTAL_FRAMES, percent), stderr)
        fflush(stderr)
    }
}
fputs("\nRendering complete! All widescreen frames streamed to encoder.\n", stderr)

