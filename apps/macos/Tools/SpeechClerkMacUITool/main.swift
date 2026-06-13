import AppKit
import ApplicationServices
import Foundation

let appName = "SpeechClerkMac"
let appBundleIdentifier = "dev.zarya.speech-clerk.macos"

@main
struct SpeechClerkMacUITool {
    static func main() throws {
        var arguments = Array(CommandLine.arguments.dropFirst())
        guard let command = arguments.first else {
            printUsage()
            return
        }
        arguments.removeFirst()

        switch command {
        case "permissions":
            print("accessibility=\(AXIsProcessTrusted() ? "trusted" : "blocked")")
        case "prompt-permissions":
            let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
            print("accessibility=\(AXIsProcessTrustedWithOptions(options) ? "trusted" : "blocked")")
        case "tree":
            let maxDepth = parseMaxDepth(arguments) ?? 6
            let app = try runningApp()
            let root = AXUIElementCreateApplication(app.processIdentifier)
            printTree(root: root, maxDepth: maxDepth)
        case "find":
            guard let query = arguments.first else {
                throw ToolError.missingArgument("find requires an identifier, title, or value")
            }
            let match = try findElement(query: query)
            print(describe(match))
        case "press":
            guard let query = arguments.first else {
                throw ToolError.missingArgument("press requires an identifier, title, or value")
            }
            let match = try findElement(query: query)
            let error = AXUIElementPerformAction(match.element, kAXPressAction as CFString)
            guard error == .success else {
                throw ToolError.accessibility("press failed with \(error)")
            }
            print("pressed=\(query)")
        case "set-text":
            guard arguments.count >= 2 else {
                throw ToolError.missingArgument("set-text requires an identifier and text value")
            }
            let query = arguments[0]
            let text = arguments.dropFirst().joined(separator: " ")
            let match = try findElement(query: query)
            let error = AXUIElementSetAttributeValue(match.element, kAXValueAttribute as CFString, text as CFTypeRef)
            guard error == .success else {
                throw ToolError.accessibility("set-text failed with \(error)")
            }
            print("set-text=\(query)")
        case "value":
            guard let query = arguments.first else {
                throw ToolError.missingArgument("value requires an identifier, title, or value")
            }
            let match = try findElement(query: query)
            print(attributeString(match.element, kAXValueAttribute) ?? "")
        case "-h", "--help", "help":
            printUsage()
        default:
            throw ToolError.missingArgument("unknown command: \(command)")
        }
    }

    private static func parseMaxDepth(_ arguments: [String]) -> Int? {
        guard let index = arguments.firstIndex(of: "--max-depth"),
            arguments.indices.contains(index + 1)
        else {
            return nil
        }

        return Int(arguments[index + 1])
    }

    private static func runningApp() throws -> NSRunningApplication {
        guard
            let app = NSWorkspace.shared.runningApplications.first(where: { candidate in
                candidate.bundleIdentifier == appBundleIdentifier
                    || candidate.localizedName == appName
                    || candidate.executableURL?.lastPathComponent == appName
            })
        else {
            throw ToolError.appNotRunning
        }

        if #available(macOS 14.0, *) {
            app.activate()
        } else {
            app.activate(options: [.activateIgnoringOtherApps])
        }
        return app
    }

    private static func findElement(query: String) throws -> ElementMatch {
        guard AXIsProcessTrusted() else {
            throw ToolError.accessibility("Accessibility permission is required to inspect \(appName)")
        }

        let app = try runningApp()
        let root = AXUIElementCreateApplication(app.processIdentifier)
        if let match = firstMatch(root: root, query: query, maxDepth: 12) {
            return match
        }

        throw ToolError.notFound(query)
    }

    private static func printTree(root: AXUIElement, maxDepth: Int) {
        guard AXIsProcessTrusted() else {
            print("accessibility=blocked")
            return
        }

        var visited = Set<CFHashCode>()
        walk(root, depth: 0, maxDepth: maxDepth, visited: &visited) { depth, element in
            print("\(String(repeating: "  ", count: depth))\(describe(ElementMatch(element: element, depth: depth)))")
        }
    }

    private static func firstMatch(root: AXUIElement, query: String, maxDepth: Int) -> ElementMatch? {
        var visited = Set<CFHashCode>()
        var match: ElementMatch?
        walk(root, depth: 0, maxDepth: maxDepth, visited: &visited) { depth, element in
            guard match == nil else {
                return
            }

            if elementMatches(element, query: query) {
                match = ElementMatch(element: element, depth: depth)
            }
        }
        return match
    }

    private static func walk(
        _ element: AXUIElement,
        depth: Int,
        maxDepth: Int,
        visited: inout Set<CFHashCode>,
        visit: (Int, AXUIElement) -> Void
    ) {
        let hash = CFHash(element)
        guard !visited.contains(hash), depth <= maxDepth else {
            return
        }
        visited.insert(hash)
        visit(depth, element)

        guard depth < maxDepth else {
            return
        }

        for child in childElements(element) {
            walk(child, depth: depth + 1, maxDepth: maxDepth, visited: &visited, visit: visit)
        }
    }

    private static func childElements(_ element: AXUIElement) -> [AXUIElement] {
        let childAttributes = [
            kAXWindowsAttribute,
            kAXChildrenAttribute,
            kAXRowsAttribute,
            kAXColumnsAttribute,
            kAXVisibleChildrenAttribute,
        ]

        var children = [AXUIElement]()
        for attribute in childAttributes {
            guard let values = attributeValue(element, attribute) as? [AXUIElement] else {
                continue
            }
            children.append(contentsOf: values)
        }
        return children
    }

    private static func elementMatches(_ element: AXUIElement, query: String) -> Bool {
        let fields = [
            attributeString(element, kAXIdentifierAttribute),
            attributeString(element, kAXTitleAttribute),
            attributeString(element, kAXDescriptionAttribute),
            attributeString(element, kAXValueAttribute),
        ]

        return fields.contains { value in
            value == query
        }
    }

    private static func describe(_ match: ElementMatch) -> String {
        let element = match.element
        return [
            "role=\(attributeString(element, kAXRoleAttribute) ?? "-")",
            "id=\(attributeString(element, kAXIdentifierAttribute) ?? "-")",
            "title=\(attributeString(element, kAXTitleAttribute) ?? "-")",
            "value=\(truncated(attributeString(element, kAXValueAttribute) ?? "-"))",
            "description=\(attributeString(element, kAXDescriptionAttribute) ?? "-")",
        ].joined(separator: " ")
    }

    private static func attributeString(_ element: AXUIElement, _ attribute: String) -> String? {
        guard let value = attributeValue(element, attribute) else {
            return nil
        }

        if let string = value as? String {
            return string
        }
        if let number = value as? NSNumber {
            return number.stringValue
        }
        return nil
    }

    private static func attributeValue(_ element: AXUIElement, _ attribute: String) -> AnyObject? {
        var value: CFTypeRef?
        let error = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
        guard error == .success else {
            return nil
        }
        return value as AnyObject?
    }

    private static func truncated(_ value: String, limit: Int = 80) -> String {
        guard value.count > limit else {
            return value
        }
        return "\(value.prefix(limit))..."
    }

    private static func printUsage() {
        print(
            """
            Usage: SpeechClerkMacUITool <command>

            Commands:
              permissions
              prompt-permissions
              tree [--max-depth N]
              find <identifier-or-title>
              press <identifier-or-title>
              set-text <identifier-or-title> <text>
              value <identifier-or-title>
            """
        )
    }
}

struct ElementMatch {
    let element: AXUIElement
    let depth: Int
}

enum ToolError: Error, CustomStringConvertible {
    case accessibility(String)
    case appNotRunning
    case missingArgument(String)
    case notFound(String)

    var description: String {
        switch self {
        case let .accessibility(message):
            return message
        case .appNotRunning:
            return "\(appName) is not running"
        case let .missingArgument(message):
            return message
        case let .notFound(query):
            return "no UI element found for \(query)"
        }
    }
}
