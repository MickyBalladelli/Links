import Foundation
import Security

final class IOSLocalDevelopmentURLSessionDelegate: NSObject, URLSessionDelegate {
    private let endpointHost: String
    private let rootCertificate: SecCertificate

    private init?(baseURL: URL, rootCertificateData: Data) {
        guard baseURL.scheme?.lowercased() == "https",
              let endpointHost = baseURL.host?.lowercased(),
              Self.isLocalDevelopmentHost(endpointHost),
              let rootCertificate = SecCertificateCreateWithData(nil, rootCertificateData as CFData) else {
            return nil
        }
        self.endpointHost = endpointHost
        self.rootCertificate = rootCertificate
    }

    static func makeURLSession(baseURL: URL, rootCertificateData: Data?) -> URLSession {
        guard let rootCertificateData,
              let delegate = Self(baseURL: baseURL, rootCertificateData: rootCertificateData) else {
            return .shared
        }

        let configuration = URLSessionConfiguration.default
        configuration.waitsForConnectivity = false
        configuration.timeoutIntervalForRequest = 15
        configuration.timeoutIntervalForResource = 30
        return URLSession(configuration: configuration, delegate: delegate, delegateQueue: nil)
    }

    func urlSession(_ session: URLSession,
                    didReceive challenge: URLAuthenticationChallenge,
                    completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        guard challenge.protectionSpace.authenticationMethod == NSURLAuthenticationMethodServerTrust,
              challenge.protectionSpace.host.lowercased() == endpointHost,
              let serverTrust = challenge.protectionSpace.serverTrust else {
            completionHandler(.performDefaultHandling, nil)
            return
        }

        let policy = SecPolicyCreateSSL(true, "links-mac.local" as CFString)
        let anchors: NSArray = [rootCertificate]
        guard SecTrustSetPolicies(serverTrust, policy) == errSecSuccess,
              SecTrustSetAnchorCertificates(serverTrust, anchors) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(serverTrust, true) == errSecSuccess else {
            completionHandler(.cancelAuthenticationChallenge, nil)
            return
        }

        var trustError: CFError?
        if SecTrustEvaluateWithError(serverTrust, &trustError) {
            completionHandler(.useCredential, URLCredential(trust: serverTrust))
        } else {
            completionHandler(.cancelAuthenticationChallenge, nil)
        }
    }

    private static func isLocalDevelopmentHost(_ host: String) -> Bool {
        guard host != "localhost", host != "127.0.0.1", host != "::1" else {
            return false
        }
        if host == "links-mac.local" {
            return true
        }
        let octets = host.split(separator: ".").compactMap { Int($0) }
        guard octets.count == 4, octets.allSatisfy({ (0...255).contains($0) }) else {
            return false
        }
        return octets[0] == 10
            || (octets[0] == 172 && (16...31).contains(octets[1]))
            || (octets[0] == 192 && octets[1] == 168)
            || (octets[0] == 169 && octets[1] == 254)
    }
}
