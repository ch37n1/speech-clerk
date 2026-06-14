import AppKit
import ApplicationServices

final class ActiveApplicationTracker {
    private(set) var lastExternalApplication: NSRunningApplication?
    private let currentBundleIdentifier = Bundle.main.bundleIdentifier

    init() {
        NSWorkspace.shared.notificationCenter.addObserver(
            self,
            selector: #selector(applicationDidActivate(_:)),
            name: NSWorkspace.didActivateApplicationNotification,
            object: nil
        )
    }

    deinit {
        NSWorkspace.shared.notificationCenter.removeObserver(self)
    }

    @objc private func applicationDidActivate(_ notification: Notification) {
        guard let application = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
            application.bundleIdentifier != currentBundleIdentifier
        else {
            return
        }

        lastExternalApplication = application
    }
}

@MainActor
enum ClipboardInserter {
    static var hasKeyboardPastePermission: Bool {
        AXIsProcessTrusted()
    }

    @discardableResult
    static func requestKeyboardPastePermission() -> Bool {
        let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
        return AXIsProcessTrustedWithOptions(options)
    }

    @discardableResult
    static func paste(_ text: String, into targetApplication: NSRunningApplication?) -> Bool {
        guard hasKeyboardPastePermission else {
            requestKeyboardPastePermission()
            return false
        }

        let pasteboard = NSPasteboard.general
        let previousSnapshot = ClipboardSnapshot.capture(from: pasteboard)

        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        let transcriptChangeCount = pasteboard.changeCount

        if #available(macOS 14.0, *) {
            targetApplication?.activate()
        } else {
            targetApplication?.activate(options: [.activateIgnoringOtherApps])
        }

        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 180_000_000)
            sendCommandV()

            try? await Task.sleep(nanoseconds: 350_000_000)
            let pasteboard = NSPasteboard.general
            if pasteboard.changeCount == transcriptChangeCount {
                previousSnapshot.restore(to: pasteboard)
            }
        }

        return true
    }

    private static func sendCommandV() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let keyCodeForV: CGKeyCode = 9
        let keyDown = CGEvent(keyboardEventSource: source, virtualKey: keyCodeForV, keyDown: true)
        let keyUp = CGEvent(keyboardEventSource: source, virtualKey: keyCodeForV, keyDown: false)

        keyDown?.flags = .maskCommand
        keyUp?.flags = .maskCommand
        keyDown?.post(tap: .cghidEventTap)
        keyUp?.post(tap: .cghidEventTap)
    }
}

private struct ClipboardSnapshot: Sendable {
    let items: [ClipboardItem]

    static func capture(from pasteboard: NSPasteboard) -> Self {
        let copiedItems =
            pasteboard.pasteboardItems?
            .compactMap(copyItem)
            ?? []
        return Self(items: copiedItems)
    }

    func restore(to pasteboard: NSPasteboard) {
        pasteboard.clearContents()
        if !items.isEmpty {
            pasteboard.writeObjects(items.map(\.pasteboardItem))
        }
    }

    private static func copyItem(_ item: NSPasteboardItem) -> ClipboardItem? {
        var representations: [ClipboardRepresentation] = []
        for pasteboardType in item.types {
            let type = pasteboardType.rawValue
            if let data = item.data(forType: pasteboardType) {
                representations.append(ClipboardRepresentation(type: type, data: data))
            }
        }

        return representations.isEmpty ? nil : ClipboardItem(representations: representations)
    }
}

private struct ClipboardItem: Sendable {
    let representations: [ClipboardRepresentation]

    var pasteboardItem: NSPasteboardItem {
        let item = NSPasteboardItem()
        for representation in representations {
            item.setData(
                representation.data,
                forType: NSPasteboard.PasteboardType(representation.type)
            )
        }
        return item
    }
}

private struct ClipboardRepresentation: Sendable {
    let type: String
    let data: Data
}
