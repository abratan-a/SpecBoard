import Foundation

/// An error as the screen says it (`specboard://SpecBoard/Docs/Design:Localization` §2): the app's own and the
/// core's are said in the screen's language already; one of the system's (Foundation's files, the network) is
/// said by macOS in the Mac's language, so it is said here in the screen's, with the system's code for it.
nonisolated enum ErrorText {
    /// The domains whose errors macOS words in the Mac's language.
    static let systemDomains: Set<String> = [NSCocoaErrorDomain, NSURLErrorDomain, NSPOSIXErrorDomain,
                                             NSOSStatusErrorDomain, NSMachErrorDomain, "kCFErrorDomainCFNetwork"]

    static func said(_ error: Error, in code: String = Language.current) -> String {
        if case .Open(let message)? = error as? CoreError { return message }
        if let network = error as? URLError { return said(network, in: code) }
        let system = error as NSError
        guard systemDomains.contains(system.domain) else {
            return (error as? LocalizedError)?.errorDescription ?? String(describing: error)
        }
        return L10n.text(.Common.errorSystem(Self.code(system)), in: code)
    }

    /// A network error by what it means to whoever waits on it.
    static func said(_ error: URLError, in code: String) -> String {
        switch error.code {
        case .notConnectedToInternet, .networkConnectionLost, .dataNotAllowed, .internationalRoamingOff:
            return L10n.text(.Common.errorOffline, in: code)
        case .timedOut:
            return L10n.text(.Common.errorTimedOut, in: code)
        case .cannotFindHost, .cannotConnectToHost, .dnsLookupFailed:
            return L10n.text(.Common.errorHostUnreachable(error.failingURL?.host() ?? Self.code(error as NSError)), in: code)
        case .secureConnectionFailed, .serverCertificateUntrusted, .serverCertificateHasBadDate,
             .serverCertificateNotYetValid, .serverCertificateHasUnknownRoot, .clientCertificateRejected:
            return L10n.text(.Common.errorSecure, in: code)
        default:
            return L10n.text(.Common.errorNetwork(Self.code(error as NSError)), in: code)
        }
    }

    /// The system's code for an error, the same in every language: `NSCocoaErrorDomain 513`.
    static func code(_ error: NSError) -> String { "\(error.domain) \(error.code)" }
}
