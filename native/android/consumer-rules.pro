# JNI names and vault callback methods are resolved by name from native code.
-keep class ai.links.identity.NativeIdentityBridge { *; }
-keep class ai.links.identity.NativeSandboxBridge { *; }
-keepclassmembers class ai.links.identity.HardwareSeedVault {
    public java.lang.String storeSeed(byte[]);
    public byte[] loadSeed(java.lang.String);
    public void deleteSeed(java.lang.String);
}
