# ProGuard rules for eche-btle Android library

# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep all classes in the eche.btle package
-keep class com.eche.btle.** { *; }

# Keep callback proxies (called from native code)
-keep class com.eche.btle.ScanCallbackProxy { *; }
-keep class com.eche.btle.GattCallbackProxy { *; }
-keep class com.eche.btle.AdvertiseCallbackProxy { *; }

# Keep EcheBtle main class
-keep class com.eche.btle.EcheBtle { *; }
-keep class com.eche.btle.EcheConnection { *; }
-keep class com.eche.btle.DiscoveredDevice { *; }
