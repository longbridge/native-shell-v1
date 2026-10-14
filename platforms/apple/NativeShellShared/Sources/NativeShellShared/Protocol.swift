// The binary mutation stream of the core, mirrored from
// `crates/core/src/protocol.rs`. The numeric values are the wire format: they
// must never drift from the Rust definitions.

import Foundation

public enum Wire {
    public static let magic: UInt32 = 0x3142_534E // "NSB1" little-endian
    public static let protocolVersion: UInt32 = 1
    public static let headerWords = 5
    public static let extendedNode: UInt8 = 0x80
    public static let nodeMask: UInt32 = 0x00FF_FFFF
    public static let rootNode: UInt32 = 0
}

public enum Op: UInt8 {
    case beginBatch = 1, endBatch, create, insert, remove, delete, move, setProp, setFrame, setEventMask, command
}

public enum BatchFlags {
    public static let animated: UInt32 = 1
    public static let initial: UInt32 = 2
}

@objc public enum NSHViewType: UInt32 {
    case view = 1, text, image, scroll, textInput, switchView, checkbox, slider, progress, list, listRow, page,
         sheet, dialog, segmented, datePicker, select, shape, spinner, hostComponent
}

public enum ValueKind: UInt8 {
    case null = 0, bool, i32, f32, color, str, json, f32Array
}

public enum Prop: UInt16 {
    case backgroundColor = 1, cornerRadii, borderWidths, borderColor, opacity, shadow, gradient, clipsToBounds, hidden,
         transform, animation
    case attributedText = 20, selectable, linksEnabled
    case imageSource = 30, contentMode, tintColor
    case horizontal = 40, contentSize, refreshing, showsIndicators, refreshEnabled, paging
    case textValue = 50, placeholder, multiline, secure, keyboardType, maxLength, allowedChars, editable, returnKey,
         textStyle, contentType, placeholderColor, autoCapitalize, contentInsets
    case checked = 70, enabled, checkStyle
    case value = 80, min, max, step, indeterminate
    case itemCount = 90, rowOffsets, rowIndex, prefetch, listAlignment
    case title = 100, canGoBack, navBarHidden, detents, dismissable, alertSpec
    case items = 110, selectedIndex, date, dateMode, menu
    case pathData = 120, fillColor, strokeColor, strokeWidth, viewBox, fillRule
    case a11yLabel = 130, a11yRole, a11yHint, a11yValue, a11yState, testId, a11yHidden
    case activeOpacity = 140, activeBackground, hitSlop, focusable, hoverOpacity, hoverBackground, cursor, tooltip
    case hostComponent = 150, hostProps, themeTokens
}

public enum A11yRole: Int32 {
    case none = 0, button, link, header, image, text, textField, switchRole, checkbox, radio, slider, progressBar, tab,
         tabList, list, listItem, menu, alert, search
}

/// GPUI's `CursorStyle`, in its order (the `Cursor` prop).
public enum CursorStyle: Int32 {
    case arrow = 0, iBeam, crosshair, closedHand, openHand, pointingHand, resizeLeft, resizeRight, resizeLeftRight,
         resizeUp, resizeDown, resizeUpDown, resizeUpLeftDownRight, resizeUpRightDownLeft, resizeColumn, resizeRow,
         iBeamCursorForVerticalLayout, operationNotAllowed, dragLink, dragCopy, contextualMenu
}

public enum Command: UInt32 {
    case focus = 1, blur, scrollTo, scrollToIndex, showToast, openUrl, setClipboard, presentPage, dismissPage, haptic, dismissToast
}

public enum EventMask {
    public static let click: UInt32 = 1 << 0
    public static let doubleClick: UInt32 = 1 << 1
    public static let longPress: UInt32 = 1 << 2
    public static let change: UInt32 = 1 << 3
    public static let submit: UInt32 = 1 << 4
    public static let focus: UInt32 = 1 << 5
    public static let blur: UInt32 = 1 << 6
    public static let scroll: UInt32 = 1 << 7
    public static let refresh: UInt32 = 1 << 8
    public static let pan: UInt32 = 1 << 9
    public static let pinch: UInt32 = 1 << 10
    public static let link: UInt32 = 1 << 11
    public static let key: UInt32 = 1 << 12
    public static let dismiss: UInt32 = 1 << 13
    public static let action: UInt32 = 1 << 14
    public static let pressIn: UInt32 = 1 << 15
    public static let pressOut: UInt32 = 1 << 16
    public static let hostEvent: UInt32 = 1 << 17
    public static let hover: UInt32 = 1 << 18
}

public enum EventType: UInt32 {
    case click = 1, doubleClick, longPress, change, submit, focus, blur, scroll, refresh, pan, pinch, link, key,
         dismiss, action, rangeChange, rowHeight, backPressed, pressIn, pressOut, hostEvent, toastAction, hover, resize
}

public enum PropValue: Equatable {
    case null
    case bool(Bool)
    case i32(Int32)
    case f32(Float)
    case color(UInt32)
    case str(String)
    case json(String)
    case f32Array([Float])

    public var kind: ValueKind {
        switch self {
        case .null: return .null
        case .bool: return .bool
        case .i32: return .i32
        case .f32: return .f32
        case .color: return .color
        case .str: return .str
        case .json: return .json
        case .f32Array: return .f32Array
        }
    }
}

public struct Frame: Equatable {
    public var x: Float, y: Float, width: Float, height: Float

    public init(x: Float, y: Float, width: Float, height: Float) {
        self.x = x
        self.y = y
        self.width = width
        self.height = height
    }

    public var cgRect: CGRect {
        CGRect(x: CGFloat(x), y: CGFloat(y), width: CGFloat(width), height: CGFloat(height))
    }
}

public enum Mutation: Equatable {
    case beginBatch(seq: UInt32, flags: UInt32)
    case endBatch
    case create(node: UInt32, type: NSHViewType)
    case insert(node: UInt32, parent: UInt32, index: UInt32)
    case remove(node: UInt32, parent: UInt32, index: UInt32)
    case delete(node: UInt32)
    case move(node: UInt32, parent: UInt32, from: UInt32, to: UInt32)
    case setProp(node: UInt32, prop: Prop, value: PropValue)
    case setFrame(node: UInt32, frame: Frame)
    case setEventMask(node: UInt32, mask: UInt32)
    case command(node: UInt32, command: Command, payload: String)
}
