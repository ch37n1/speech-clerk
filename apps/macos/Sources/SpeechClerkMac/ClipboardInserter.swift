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
        let previousString = pasteboard.string(forType: .string)

        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        let transcriptChangeCount = pasteboard.changeCount

        if #available(macOS 14.0, *) {
            targetApplication?.activate()
        } else {
            targetApplication?.activate(options: [.activateIgnoringOtherApps])
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.18) {
            sendCommandV()

            DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                if pasteboard.changeCount == transcriptChangeCount {
                    pasteboard.clearContents()
                    if let previousString {
                        pasteboard.setString(previousString, forType: .string)
                    }
                }
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
