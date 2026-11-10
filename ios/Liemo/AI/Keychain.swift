import Foundation
import Security

/// The Anthropic API key, kept in the Keychain (this device only).
enum Keychain {
    private static let service = "dev.flaxo.liemo.anthropic"

    static var apiKey: String? {
        get {
            let q: [CFString: Any] = [kSecClass: kSecClassGenericPassword, kSecAttrService: service, kSecReturnData: true, kSecMatchLimit: kSecMatchLimitOne]
            var out: CFTypeRef?
            guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let d = out as? Data else { return nil }
            return String(data: d, encoding: .utf8)
        }
        set {
            let q: [CFString: Any] = [kSecClass: kSecClassGenericPassword, kSecAttrService: service]
            SecItemDelete(q as CFDictionary)
            guard let v = newValue, !v.isEmpty else { return }
            var add = q
            add[kSecValueData] = Data(v.utf8)
            add[kSecAttrAccessible] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            SecItemAdd(add as CFDictionary, nil)
        }
    }
}
