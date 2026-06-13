import SwiftUI

@main
struct SpeechClerkMacApp: App {
    @StateObject private var viewModel = DictationViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
                .frame(minWidth: 520, minHeight: 460)
        }
        .commands {
            CommandGroup(replacing: .newItem) {}
        }
    }
}
