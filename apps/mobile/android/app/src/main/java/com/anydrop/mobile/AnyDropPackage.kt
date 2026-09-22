package com.anydrop.mobile

import com.facebook.react.BaseReactPackage
import com.facebook.react.bridge.NativeModule
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.module.model.ReactModuleInfo
import com.facebook.react.module.model.ReactModuleInfoProvider

class AnyDropPackage : BaseReactPackage() {
  override fun getModule(name: String, context: ReactApplicationContext): NativeModule? = if (name == NativeAnyDropModule.NAME) NativeAnyDropModule(context) else null
  override fun getReactModuleInfoProvider() = ReactModuleInfoProvider {
    mapOf(NativeAnyDropModule.NAME to ReactModuleInfo(NativeAnyDropModule.NAME, NativeAnyDropModule.NAME, false, false, false, true))
  }
}
