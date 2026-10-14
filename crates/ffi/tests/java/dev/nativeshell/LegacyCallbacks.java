package dev.nativeshell;

/** A callbacks object from before `measureTextBatch` existed (an older library). */
public final class LegacyCallbacks {
    public int singleMeasures;

    public synchronized float[] measureText(String attributedJson, float width, int widthMode) {
        singleMeasures++;
        return TestCallbacks.metrics(attributedJson);
    }

    public float[] measureComponent(String name, String propsJson, float width, int widthMode) { return null; }
    public void batchReady() {}
    public String callSync(String module, String method, String argsJson) { return "null"; }
    public void callAsync(long requestId, String module, String method, String argsJson) {}
    public void log(int level, String message) {}
    public void error(String message) {}
    public synchronized int singles() { return singleMeasures; }
}
