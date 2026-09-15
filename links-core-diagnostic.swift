import Foundation
import CLinksDesktopClient

func sign(_ context: UnsafeMutableRawPointer?, _ bytes: UnsafePointer<UInt8>?, _ length: Int, _ output: UnsafeMutablePointer<UInt8>?) -> Int32 { 0 }
func store(_ context: UnsafeMutableRawPointer?, _ key: UnsafePointer<UInt8>?, _ keyLength: Int, _ secret: UnsafePointer<UInt8>?, _ secretLength: Int) -> Int32 { 0 }
func load(_ context: UnsafeMutableRawPointer?, _ key: UnsafePointer<UInt8>?, _ keyLength: Int, _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int, _ outputLength: UnsafeMutablePointer<Int>?) -> Int32 { 1 }
func delete(_ context: UnsafeMutableRawPointer?, _ key: UnsafePointer<UInt8>?, _ keyLength: Int) -> Int32 { 0 }
func loadState(_ context: UnsafeMutableRawPointer?, _ output: UnsafeMutablePointer<UInt8>?, _ capacity: Int, _ outputLength: UnsafeMutablePointer<Int>?) -> Int32 { outputLength?.pointee = 0; return 0 }
func saveState(_ context: UnsafeMutableRawPointer?, _ bytes: UnsafePointer<UInt8>?, _ length: Int) -> Int32 { 0 }
func sendFrame(_ context: UnsafeMutableRawPointer?, _ bytes: UnsafePointer<UInt8>?, _ length: Int) -> Int32 { 0 }

let path = "/Users/micky/Library/Containers/ai.links.Links/Data/Library/Preferences/ai.links.client.profile.a03e727d53467ef7322078402d54293fa0fc3598c30a5d8d8b8c8b25f6cc6ab0.plist"
let outer = try PropertyListSerialization.propertyList(from: Data(contentsOf: URL(fileURLWithPath: path)), options: [], format: nil) as! [String: Any]
let inner = outer["links.client.metadata.v1.alice"] as! Data
let metadata = try PropertyListSerialization.propertyList(from: inner, options: [], format: nil) as! [String: Any]
let user = metadata["userID"] as! String
let device = metadata["deviceID"] as! String
let credential = metadata["mlsCredential"] as! Data
let key = metadata["publicKey"] as! Data
var callbacks = LinksDesktopCoreCallbacks()
callbacks.abi_version = 1
callbacks.sign = sign
callbacks.store_secret = store
callbacks.load_secret = load
callbacks.delete_secret = delete
callbacks.load_state = loadState
callbacks.save_state = saveState
callbacks.send_frame = sendFrame
withUnsafeMutableBytes(of: &callbacks.identity_public_key) { destination in
    destination.copyBytes(from: key)
}
var core: OpaquePointer?
let status = credential.withUnsafeBytes { credentialBytes in
    user.withCString { userBytes in
        device.withCString { deviceBytes in
            withUnsafeMutablePointer(to: &callbacks) { callbackPointer in
                links_desktop_core_create(
                    UnsafeRawPointer(userBytes).assumingMemoryBound(to: UInt8.self), user.utf8.count,
                    UnsafeRawPointer(deviceBytes).assumingMemoryBound(to: UInt8.self), device.utf8.count,
                    credentialBytes.bindMemory(to: UInt8.self).baseAddress!, credential.count,
                    callbackPointer, &core)
            }
        }
    }
}
print("status=\(status) core=\(core != nil)")
if let core { links_desktop_core_destroy(core) }
