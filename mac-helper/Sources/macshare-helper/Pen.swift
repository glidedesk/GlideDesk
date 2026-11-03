// TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
// A phone's stylus (in.* with `pen`, shared/protocol.js P.penOf) as a tablet pen: the mouse events
// Injector posts for it are of the tablet-point subtype, with its pressure, tilt and rotation, and
// proximity events say when it comes near and goes (its eraser end comes as an eraser), so drawing
// apps see a pen, not a mouse. Its tip is the left button, its side button the right one.
import Cocoa

final class PenInput {
  private static let deviceID: Int64 = 0x7D03 // ties the points to their proximity
  // Device ID, absolute X/Y, buttons, tilt X/Y, pressure, rotation (NX tablet capability bits).
  private static let capabilities: Int64 = 0x0001 | 0x0002 | 0x0004 | 0x0040 | 0x0080 | 0x0100 | 0x0400 | 0x2000
  private let source: CGEventSource?
  private let post: (CGEvent?) -> Void
  private var near = false
  private var eraser = false
  var current: [String: Any]? // the pen of the event being posted (Injector.mouse decorates it)

  init(source: CGEventSource?, post: @escaping (CGEvent?) -> Void) {
    self.source = source
    self.post = post
  }

  /// The pen's state before its event: proximity entered (or the eraser end turned to), or left.
  /// False when it left: nothing else to post.
  func arrive(_ pen: [String: Any]) -> Bool {
    if (pen["out"] as? Bool) == true {
      leave()
      return false
    }
    let er = (pen["er"] as? Bool) == true
    if near && er != eraser { leave() }
    if !near {
      eraser = er
      proximity(true)
      near = true
    }
    return true
  }

  func leave() {
    guard near else { return }
    proximity(false)
    near = false
  }

  private func proximity(_ enter: Bool) {
    guard let e = CGEvent(source: source) else { return }
    e.type = .tabletProximity
    e.setIntegerValueField(.mouseEventSubtype, value: Int64(CGEventMouseSubtype.tabletProximity.rawValue))
    e.setIntegerValueField(.tabletProximityEventVendorID, value: 0x1209)
    e.setIntegerValueField(.tabletProximityEventTabletID, value: 1)
    e.setIntegerValueField(.tabletProximityEventPointerID, value: 1)
    e.setIntegerValueField(.tabletProximityEventDeviceID, value: PenInput.deviceID)
    e.setIntegerValueField(.tabletProximityEventSystemTabletID, value: 1)
    e.setIntegerValueField(.tabletProximityEventVendorUniqueID, value: PenInput.deviceID)
    e.setIntegerValueField(.tabletProximityEventCapabilityMask, value: PenInput.capabilities)
    e.setIntegerValueField(.tabletProximityEventPointerType, value: Int64(eraser ? NSEvent.PointingDeviceType.eraser.rawValue : NSEvent.PointingDeviceType.pen.rawValue))
    e.setIntegerValueField(.tabletProximityEventEnterProximity, value: enter ? 1 : 0)
    post(e)
  }

  /// A mouse event at `p` made a pen point (pressure, tilt, rotation, its buttons).
  func decorate(_ e: CGEvent, at p: CGPoint) {
    guard let pen = current else { return }
    let num = { (k: String) in (pen[k] as? NSNumber)?.doubleValue ?? 0 }
    let pressure = (pen["c"] as? Bool) == true ? num("p") : 0
    let buttons: Int64 = ((pen["c"] as? Bool) == true ? 1 : 0) | ((pen["bar"] as? Bool) == true ? 2 : 0)
    e.setIntegerValueField(.mouseEventSubtype, value: Int64(CGEventMouseSubtype.tabletPoint.rawValue))
    e.setDoubleValueField(.mouseEventPressure, value: pressure)
    e.setIntegerValueField(.tabletEventPointX, value: Int64(p.x.rounded()))
    e.setIntegerValueField(.tabletEventPointY, value: Int64(p.y.rounded()))
    e.setIntegerValueField(.tabletEventPointButtons, value: buttons)
    e.setDoubleValueField(.tabletEventPointPressure, value: pressure)
    e.setDoubleValueField(.tabletEventTiltX, value: num("tx") / 90)
    e.setDoubleValueField(.tabletEventTiltY, value: num("ty") / 90)
    e.setDoubleValueField(.tabletEventRotation, value: num("tw"))
    e.setIntegerValueField(.tabletEventDeviceID, value: PenInput.deviceID)
  }
}
