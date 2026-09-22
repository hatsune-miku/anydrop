package com.anydrop.mobile

import com.facebook.react.bridge.Promise
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.module.annotations.ReactModule
import java.util.concurrent.Executors

@ReactModule(name = NativeAnyDropModule.NAME)
class NativeAnyDropModule(context: ReactApplicationContext) : NativeAnyDropSpec(context) {
  private val runtime = AnyDropRuntime.get(context)
  private val io = Executors.newSingleThreadExecutor()
  override fun getName() = NAME
  private fun perform(promise: Promise, action: () -> Any?) {
    try { promise.resolve(action()) } catch (error: Exception) { promise.reject("ANYDROP", error.message, error) }
  }
  override fun getSnapshot(promise: Promise) = perform(promise) { runtime.snapshot() }
  override fun configure(settings: String, promise: Promise) = perform(promise) { runtime.configure(settings) }
  override fun start(promise: Promise) = perform(promise) { runtime.start() }
  override fun stop(promise: Promise) = perform(promise) { runtime.stop() }
  override fun sendText(text: String, promise: Promise) { io.execute { perform(promise) { runtime.sendText(text) } } }
  override fun getClipboard(promise: Promise) { reactApplicationContext.runOnUiQueueThread { perform(promise) { check(reactApplicationContext.currentActivity?.hasWindowFocus() == true) { "请在应用前台粘贴文本" }; runtime.clipboard() } } }
  override fun copyText(text: String, promise: Promise) { reactApplicationContext.runOnUiQueueThread { perform(promise) { runtime.copyText(text); null } } }
  override fun invalidate() { io.shutdown(); super.invalidate() }
  companion object { const val NAME = "NativeAnyDrop" }
}
