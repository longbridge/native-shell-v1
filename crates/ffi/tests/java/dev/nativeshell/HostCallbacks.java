package dev.nativeshell;

/** Mirror of the Kotlin interface the Android library declares, for the JNI tests. */
public interface HostCallbacks {
    float[] measureText(String attributedJson, float width, int widthMode);
    float[] measureTextBatch(String[] attributedJson, float[] widths, int[] widthModes);
    float[] measureComponent(String name, String propsJson, float width, int widthMode);
    void batchReady();
    String callSync(String module, String method, String argsJson);
    void callAsync(long requestId, String module, String method, String argsJson);
    void log(int level, String message);
    void error(String message);
}
