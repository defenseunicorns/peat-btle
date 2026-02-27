# Consumer ProGuard rules for peat-btle
# These rules are applied to apps that use this library

# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep callback proxies
-keep class com.peat.btle.ScanCallbackProxy { *; }
-keep class com.peat.btle.GattCallbackProxy { *; }
-keep class com.peat.btle.AdvertiseCallbackProxy { *; }
