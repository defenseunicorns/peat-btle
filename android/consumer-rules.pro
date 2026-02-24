# Consumer ProGuard rules for eche-btle
# These rules are applied to apps that use this library

# Keep JNI native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep callback proxies
-keep class com.eche.btle.ScanCallbackProxy { *; }
-keep class com.eche.btle.GattCallbackProxy { *; }
-keep class com.eche.btle.AdvertiseCallbackProxy { *; }
