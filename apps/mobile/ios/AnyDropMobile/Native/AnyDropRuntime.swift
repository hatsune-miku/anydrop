import Foundation
import Network
import UIKit

@objcMembers final class AnyDropRuntime: NSObject, NetServiceBrowserDelegate, NetServiceDelegate {
  private let preferences = UserDefaults.standard
  private var browser: NetServiceBrowser?
  private var advertised: NetService?
  private var services: [String: NetService] = [:]
  private var peers: [String: [String: Any]] = [:]
  private var peerAddresses: [String: Set<String>] = [:]
  private var incoming: [UUID: NWConnection] = [:]
  private var outgoing: [UUID: NWConnection] = [:]
  private var listener: NWListener?
  private var running = false
  private var status = ""
  private var lastReceived = ""
  private var generation = 0
  private var group: UInt32 { UInt32(preferences.object(forKey: "group") as? UInt64 ?? 0) }
  private var displayName: String {
    preferences.string(forKey: "displayName") ?? UIDevice.current.name
  }
  private var deviceID: String {
    if let saved = preferences.string(forKey: "deviceID") { return saved }
    let value = UUID().uuidString.lowercased()
    preferences.set(value, forKey: "deviceID")
    return value
  }
  private var receiveText: Bool { preferences.object(forKey: "receiveText") as? Bool ?? true }

  override init() {
    super.init()
    applyAppearance()
  }

  private func applyAppearance() {
    guard Thread.isMainThread else {
      DispatchQueue.main.async { [weak self] in self?.applyAppearance() }
      return
    }
    let mode = preferences.string(forKey: "mode") ?? "system"
    let appearance: UIUserInterfaceStyle =
      mode == "dark" ? .dark : mode == "light" ? .light : .unspecified
    UIApplication.shared.delegate?.window??.overrideUserInterfaceStyle = appearance
    for case let scene as UIWindowScene in UIApplication.shared.connectedScenes {
      for window in scene.windows { window.overrideUserInterfaceStyle = appearance }
    }
  }

  func snapshot() -> String {
    let state: [String: Any] = [
      "running": running, "group": UInt64(group), "displayName": displayName,
      "mode": preferences.string(forKey: "mode") ?? "system", "receiveText": receiveText,
      "peers": peers.values.sorted {
        ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "")
      },
      "lastReceived": lastReceived, "status": status,
    ]
    return String(data: try! JSONSerialization.data(withJSONObject: state), encoding: .utf8)!
  }
  func configure(_ json: String) throws -> String {
    guard let data = json.data(using: .utf8),
      let settings = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      let value = settings["group"] as? NSNumber, value.doubleValue >= 0,
      value.doubleValue <= Double(UInt32.max), value.doubleValue.rounded() == value.doubleValue,
      let name = settings["displayName"] as? String,
      !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, name.count <= 48,
      let mode = settings["mode"] as? String, ["system", "light", "dark"].contains(mode),
      let receive = settings["receiveText"] as? Bool
    else { throw failure("设置无效：检查设备名和频段") }
    let restart = running && (UInt32(value.uint64Value) != group || name != displayName)
    if restart { _ = stop() }
    preferences.set(value.uint64Value, forKey: "group")
    preferences.set(name, forKey: "displayName")
    preferences.set(mode, forKey: "mode")
    preferences.set(receive, forKey: "receiveText")
    applyAppearance()
    if restart { return try start() }
    return snapshot()
  }
  func start() throws -> String {
    if running { return snapshot() }
    generation += 1
    let current = generation
    let server = try NWListener(using: .tcp, on: .any)
    listener = server
    running = true
    status = "正在发现同频段设备"
    server.stateUpdateHandler = { [weak self, weak server] state in
      guard let self, self.generation == current else { return }
      switch state {
      case .ready:
        guard let port = server?.port else { return }
        let service = NetService(
          domain: "local.", type: "_anydrop._udp.", name: self.deviceID, port: Int32(port.rawValue))
        service.delegate = self
        service.setTXTRecord(
          NetService.data(fromTXTRecord: [
            "g": Data(String(self.group).utf8), "dn": Data(self.displayName.utf8),
            "did": Data(self.deviceID.utf8), "v": Data("0.1.0".utf8),
            "cap": Data("clipboardText".utf8),
          ]))
        self.advertised = service
        service.publish()
      case .failed(let error):
        self.status = "监听失败：\(error.localizedDescription)"
        _ = self.stopKeepingStatus()
      default: break
      }
    }
    server.newConnectionHandler = { [weak self] connection in
      self?.receive(connection, generation: current)
    }
    server.start(queue: .main)
    let discovery = NetServiceBrowser()
    discovery.delegate = self
    browser = discovery
    discovery.searchForServices(ofType: "_anydrop._udp.", inDomain: "local.")
    return snapshot()
  }
  func stop() -> String {
    status = ""
    return stopKeepingStatus()
  }
  private func stopKeepingStatus() -> String {
    generation += 1
    running = false
    browser?.stop()
    browser = nil
    advertised?.stop()
    advertised = nil
    listener?.cancel()
    listener = nil
    incoming.values.forEach { $0.cancel() }
    incoming.removeAll()
    outgoing.values.forEach { $0.cancel() }
    outgoing.removeAll()
    services.values.forEach { $0.stop() }
    services.removeAll()
    peers.removeAll()
    peerAddresses.removeAll()
    return snapshot()
  }
  func netServiceBrowser(
    _ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool
  ) {
    guard self.browser === browser, service.name != deviceID, services.count < 128 else { return }
    services[service.name] = service
    service.delegate = self
    service.resolve(withTimeout: 5)
  }
  func netServiceBrowser(
    _ browser: NetServiceBrowser, didRemove service: NetService, moreComing: Bool
  ) {
    services.removeValue(forKey: service.name)?.stop()
    peers.removeValue(forKey: service.name)
    peerAddresses.removeValue(forKey: service.name)
  }
  func netServiceBrowser(_ browser: NetServiceBrowser, didNotSearch errorDict: [String: NSNumber]) {
    status = "设备发现失败：\(errorDict)"
    _ = stopKeepingStatus()
  }
  func netServiceDidResolveAddress(_ sender: NetService) {
    guard running, services[sender.name] === sender, let txt = sender.txtRecordData(),
      let host = sender.hostName, sender.port > 0
    else { return }
    let values = NetService.dictionary(fromTXTRecord: txt)
    func field(_ key: String) -> String {
      String(data: values[key] ?? Data(), encoding: .utf8) ?? ""
    }
    guard field("did") != deviceID, UInt32(field("g")) == group else {
      peers.removeValue(forKey: sender.name)
      peerAddresses.removeValue(forKey: sender.name)
      return
    }
    peerAddresses[sender.name] = Set(
      (sender.addresses ?? []).compactMap { address in
        address.withUnsafeBytes { raw -> String? in
          guard let base = raw.baseAddress else { return nil }
          var buffer = [CChar](repeating: 0, count: Int(NI_MAXHOST))
          guard
            getnameinfo(
              base.assumingMemoryBound(to: sockaddr.self), socklen_t(address.count), &buffer,
              socklen_t(buffer.count), nil, 0, NI_NUMERICHOST) == 0
          else { return nil }
          return String(cString: buffer)
        }
      })
    peers[sender.name] = [
      "id": field("did").isEmpty ? sender.name : field("did"),
      "name": field("dn").isEmpty ? sender.name : field("dn"), "host": host, "port": sender.port,
    ]
  }
  func netService(_ sender: NetService, didNotPublish errorDict: [String: NSNumber]) {
    status = "发布设备失败：\(errorDict)"
  }
  func sendText(_ text: String, completion: @escaping (String?, NSError?) -> Void) {
    guard running else {
      completion(nil, failure("请先开启服务"))
      return
    }
    guard !peers.isEmpty else {
      completion(nil, failure("没有可达的同频段设备"))
      return
    }
    let payload: Data
    do { payload = try TextWire.encode(text) } catch {
      completion(nil, error as NSError)
      return
    }
    let targets = Array(peers.values)
    let current = generation
    var remaining = targets.count
    var succeeded = 0
    var failures: [String] = []
    for peer in targets {
      guard let host = peer["host"] as? String, let port = peer["port"] as? Int else { continue }
      let connection = NWConnection(
        host: NWEndpoint.Host(host), port: NWEndpoint.Port(rawValue: UInt16(port))!, using: .tcp)
      let token = UUID()
      outgoing[token] = connection
      var finished = false
      func finish(_ error: Error?) {
        guard !finished else { return }
        finished = true
        self.outgoing.removeValue(forKey: token)
        connection.cancel()
        if let error {
          failures.append("\(peer["name"] ?? host)：\(error.localizedDescription)")
        } else {
          succeeded += 1
        }
        remaining -= 1
        if remaining == 0 {
          if self.generation == current {
            self.status =
              "已发送到 \(succeeded) 台设备"
              + (failures.isEmpty ? "" : "；" + failures.joined(separator: "；"))
          }
          completion(self.snapshot(), nil)
        }
      }
      connection.stateUpdateHandler = { state in
        switch state {
        case .ready: connection.send(content: payload, completion: .contentProcessed { finish($0) })
        case .failed(let error): finish(error)
        case .cancelled: finish(self.failure("发送已停止"))
        default: break
        }
      }
      connection.start(queue: .main)
      DispatchQueue.main.asyncAfter(deadline: .now() + 6) { finish(self.failure("连接超时")) }
    }
  }
  private func receive(_ connection: NWConnection, generation current: Int) {
    // Legacy text packets lack a channel field. Admit only resolved same-channel peers.
    guard receiveText, running, incoming.count < 8,
      case .hostPort(let remoteHost, _) = connection.endpoint,
      peerAddresses.values.contains(where: { $0.contains(String(describing: remoteHost)) })
    else {
      connection.cancel()
      return
    }
    let token = UUID()
    incoming[token] = connection
    var buffer = Data()
    var ended = false
    func finish() {
      guard !ended else { return }
      ended = true
      self.incoming.removeValue(forKey: token)
      connection.cancel()
    }
    func read() {
      connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) {
        [weak self] data, _, complete, error in
        guard let self, !ended else { return }
        if let data { buffer.append(data) }
        if buffer.count > TextWire.maxBytes + 18 {
          finish()
          return
        }
        if buffer.count >= 4 {
          let length = TextWire.u32(buffer, 0)
          if length < 14 || length > TextWire.maxBytes + 14 {
            finish()
            return
          }
          if buffer.count >= length + 4 {
            defer { finish() }
            guard self.running, self.generation == current, self.receiveText,
              let text = try? TextWire.decode(buffer)
            else { return }
            self.lastReceived = text
            self.status = "收到同频段文本"
            if UIApplication.shared.applicationState == .active {
              UIPasteboard.general.string = text
            }
            return
          }
        }
        if complete || error != nil { finish() } else { read() }
      }
    }
    connection.start(queue: .main)
    read()
    DispatchQueue.main.asyncAfter(deadline: .now() + 8) { finish() }
  }
  func clipboard() throws -> String { throw failure("请使用系统粘贴按钮或文本框的粘贴菜单") }
  func copyText(_ text: String) { UIPasteboard.general.string = text }
  private func failure(_ message: String) -> NSError {
    NSError(domain: "AnyDrop", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
  }
}
