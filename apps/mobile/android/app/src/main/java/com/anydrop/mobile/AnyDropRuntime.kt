package com.anydrop.mobile

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject
import java.net.ServerSocket
import java.net.Socket
import java.net.InetSocketAddress
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.Semaphore

class AnyDropRuntime private constructor(private val context: Context) {
  private val prefs = context.getSharedPreferences("anydrop", Context.MODE_PRIVATE)
  private val nsd = context.getSystemService(NsdManager::class.java)
  private val io = Executors.newFixedThreadPool(4)
  private val incomingSlots = Semaphore(8)
  private val peers = linkedMapOf<String, JSONObject>()
  private var discovery: NsdManager.DiscoveryListener? = null
  private var registration: NsdManager.RegistrationListener? = null
  private var server: ServerSocket? = null
  private var running = false
  private var generation = 0
  private var status = ""
  private var lastReceived = ""
  private val group get() = prefs.getLong("group", 0)
  private val displayName get() = prefs.getString("displayName", Build.MODEL)!!
  private val receiveText get() = prefs.getBoolean("receiveText", true)
  private val deviceID: String = prefs.getString("deviceID", null) ?: UUID.randomUUID().toString().also { prefs.edit().putString("deviceID", it).apply() }

  @Synchronized fun snapshot(): String = JSONObject().put("running", running).put("group", group).put("displayName", displayName)
    .put("mode", prefs.getString("mode", "system")).put("receiveText", receiveText).put("peers", JSONArray(peers.values.toList()))
    .put("status", status).put("lastReceived", lastReceived).toString()

  @Synchronized fun configure(json: String): String {
    val settings = JSONObject(json)
    val nextGroup = settings.getDouble("group")
    val name = settings.getString("displayName").trim()
    val mode = settings.getString("mode")
    require(nextGroup >= 0 && nextGroup <= 4294967295.0 && nextGroup % 1 == 0.0) { "频段必须是 0 到 4294967295 之间的整数" }
    require(name.isNotEmpty() && name.length <= 48) { "设备名需为 1 到 48 个字符" }
    require(mode in listOf("system", "light", "dark")) { "外观设置无效" }
    val restart = running && (nextGroup.toLong() != group || name != displayName)
    if (restart) stop()
    prefs.edit().putLong("group", nextGroup.toLong()).putString("displayName", name).putString("mode", mode)
      .putBoolean("receiveText", settings.getBoolean("receiveText")).apply()
    return if (restart) start() else snapshot()
  }

  @Synchronized fun start(): String {
    if (running) return snapshot()
    val socket = ServerSocket(0)
    server = socket; running = true; generation += 1; status = "正在发现同频段设备"
    val current = generation
    val announcement = NsdServiceInfo().apply {
      serviceName = deviceID; serviceType = "_anydrop._udp."; port = socket.localPort
      setAttribute("g", group.toString()); setAttribute("dn", displayName); setAttribute("did", deviceID)
      setAttribute("v", "0.1.0"); setAttribute("cap", "clipboardText")
    }
    val publish = object : NsdManager.RegistrationListener {
      override fun onServiceRegistered(info: NsdServiceInfo) = Unit
      override fun onServiceUnregistered(info: NsdServiceInfo) = Unit
      override fun onUnregistrationFailed(info: NsdServiceInfo, code: Int) = Unit
      override fun onRegistrationFailed(info: NsdServiceInfo, code: Int) { synchronized(this@AnyDropRuntime) { if (generation == current) status = "发布设备失败：NSD $code" } }
    }
    registration = publish
    val listener = object : NsdManager.DiscoveryListener {
      override fun onDiscoveryStarted(type: String) = Unit
      override fun onDiscoveryStopped(type: String) = Unit
      override fun onStopDiscoveryFailed(type: String, code: Int) = Unit
      override fun onStartDiscoveryFailed(type: String, code: Int) { synchronized(this@AnyDropRuntime) { if (generation == current) { stop(); status = "设备发现失败：NSD $code" } } }
      override fun onServiceLost(info: NsdServiceInfo) { synchronized(this@AnyDropRuntime) { if (generation == current) peers.remove(info.serviceName) } }
      override fun onServiceFound(info: NsdServiceInfo) {
        if (info.serviceName == deviceID) return
        nsd.resolveService(info, io, object : NsdManager.ResolveListener {
          override fun onResolveFailed(service: NsdServiceInfo, code: Int) = Unit
          override fun onServiceResolved(service: NsdServiceInfo) {
            synchronized(this@AnyDropRuntime) {
              if (!running || generation != current) return
              fun field(name: String) = service.attributes[name]?.toString(Charsets.UTF_8) ?: ""
              if (field("did") == deviceID || field("g").toLongOrNull() != group) { peers.remove(service.serviceName); return }
              val host = service.host?.hostAddress ?: return
              if (peers.size >= 128 && !peers.containsKey(service.serviceName)) return
              peers[service.serviceName] = JSONObject().put("id", field("did").ifEmpty { service.serviceName })
                .put("name", field("dn").ifEmpty { service.serviceName }).put("host", host).put("port", service.port)
            }
          }
        })
      }
    }
    discovery = listener
    try {
      nsd.registerService(announcement, NsdManager.PROTOCOL_DNS_SD, publish)
      nsd.discoverServices("_anydrop._udp.", NsdManager.PROTOCOL_DNS_SD, listener)
    } catch (error: Exception) { stop(); throw error }
    Thread({
      while (!socket.isClosed) {
        val incoming = try { socket.accept() } catch (_: Exception) { break }
        val allowed = synchronized(this) { running && generation == current && receiveText && peers.values.any { it.optString("host") == incoming.inetAddress.hostAddress } }
        if (!allowed || !incomingSlots.tryAcquire()) { incoming.close(); continue }
        io.execute {
          try {
            incoming.use { client ->
              client.soTimeout = 5000
              val input = client.getInputStream()
              val header = input.readNBytes(4)
              require(header.size == 4)
              val count = TextWire.intLE(header, 0)
              require(count in 14..65549)
              val body = input.readNBytes(count)
              require(body.size == count)
              val text = TextWire.decode(header + body)
              synchronized(this) {
                if (running && generation == current && receiveText) {
                  lastReceived = text; status = "收到同频段文本"
                  try { copyText(text) } catch (error: Exception) { status = "文本已收到，复制失败：${error.message}" }
                }
              }
            }
          } catch (_: Exception) { incoming.close() } finally { incomingSlots.release() }
        }
      }
    }, "anydrop-text-listener").start()
    return snapshot()
  }

  @Synchronized fun stop(): String {
    generation += 1; running = false
    discovery?.let { try { nsd.stopServiceDiscovery(it) } catch (_: IllegalArgumentException) {} }; discovery = null
    registration?.let { try { nsd.unregisterService(it) } catch (_: IllegalArgumentException) {} }; registration = null
    server?.close(); server = null; peers.clear(); status = ""
    return snapshot()
  }

  fun sendText(text: String): String {
    val packet = TextWire.encode(text)
    val current: Int
    val targets: List<JSONObject>
    synchronized(this) { check(running) { "请先开启服务" }; check(peers.isNotEmpty()) { "没有可达的同频段设备" }; targets = peers.values.toList(); current = generation }
    var sent = 0
    val errors = mutableListOf<String>()
    for (peer in targets) {
      if (synchronized(this) { !running || generation != current }) break
      try {
        Socket().use { socket ->
          socket.connect(InetSocketAddress(peer.getString("host"), peer.getInt("port")), 3000)
          socket.soTimeout = 3000
          socket.getOutputStream().write(packet); socket.getOutputStream().flush(); sent += 1
        }
      } catch (error: Exception) { errors.add("${peer.optString("name")}：${error.message}") }
    }
    synchronized(this) { if (generation == current) status = "已发送到 $sent 台设备" + if (errors.isEmpty()) "" else "；" + errors.joinToString("；") }
    return snapshot()
  }
  fun clipboard(): String = context.getSystemService(ClipboardManager::class.java).primaryClip?.let { if (it.itemCount > 0) it.getItemAt(0).coerceToText(context).toString() else "" } ?: ""
  fun copyText(text: String) { context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("AnyDrop", text)) }

  companion object {
    @Volatile private var instance: AnyDropRuntime? = null
    fun get(context: Context): AnyDropRuntime = instance ?: synchronized(this) { instance ?: AnyDropRuntime(context.applicationContext).also { instance = it } }
  }
}
