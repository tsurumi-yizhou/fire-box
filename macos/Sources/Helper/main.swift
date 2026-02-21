import AppKit
import Foundation

private enum ExitCode: Int32 {
    case approved = 0
    case denied = 1
    case error = 2
}

private struct Strings {
    let title: String
    let instruction: String
    let content: String
    let allow: String
    let cancel: String
}

private func requesterName() -> String {
    let args = CommandLine.arguments
    if args.count > 1, !args[1].isEmpty {
        return args[1]
    }

    return NSLocalizedString("helper.default_requester", bundle: .module, value: "An application", comment: "")
}

private func localizedStrings() -> Strings {
    let name = requesterName()
    let instructionFormat = NSLocalizedString(
        "helper.instruction_format",
        bundle: .module,
        value: "%@ wants to use AI capabilities. Approve?",
        comment: ""
    )

    return Strings(
        title: NSLocalizedString("helper.title", bundle: .module, value: "AI Capability Request", comment: ""),
        instruction: String(format: instructionFormat, locale: Locale.current, name),
        content: NSLocalizedString("helper.content", bundle: .module, value: "This request is sent by the local AI capability management service.", comment: ""),
        allow: NSLocalizedString("helper.allow", bundle: .module, value: "Allow", comment: ""),
        cancel: NSLocalizedString("helper.cancel", bundle: .module, value: "Cancel", comment: "")
    )
}

private let strings = localizedStrings()

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
app.activate(ignoringOtherApps: true)

let alert = NSAlert()
alert.alertStyle = .warning
alert.messageText = strings.instruction
alert.informativeText = strings.content
alert.window.title = strings.title
alert.addButton(withTitle: strings.allow)
alert.addButton(withTitle: strings.cancel)

let result = alert.runModal()
exit(result == .alertFirstButtonReturn ? ExitCode.approved.rawValue : ExitCode.denied.rawValue)
