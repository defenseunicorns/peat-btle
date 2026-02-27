# ProGuard rules for peat-btle Android library

# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep all classes in the peat.btle package
-keep class com.peat.btle.** { *; }

# Keep callback proxies (called from native code)
-keep class com.peat.btle.ScanCallbackProxy { *; }
-keep class com.peat.btle.GattCallbackProxy { *; }
-keep class com.peat.btle.AdvertiseCallbackProxy { *; }

# Keep PeatBtle main class
-keep class com.peat.btle.PeatBtle { *; }
-keep class com.peat.btle.PeatConnection { *; }
-keep class com.peat.btle.DiscoveredDevice { *; }
