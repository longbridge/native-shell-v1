# JNI entry points and callbacks are looked up by name from native code.
-keep class dev.nativeshell.NativeBridge { *; }
-keep interface dev.nativeshell.HostCallbacks { *; }
-keep class * implements dev.nativeshell.HostCallbacks { *; }
