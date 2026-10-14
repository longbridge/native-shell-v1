package dev.nativeshell;

import java.util.ArrayList;
import java.util.List;

/** Records what the native side calls; answers host-module calls for the JNI tests. */
public final class TestCallbacks implements HostCallbacks {
    public final List<String> errors = new ArrayList<>();
    public final List<String> logs = new ArrayList<>();
    public final List<String> asyncCalls = new ArrayList<>();
    public final List<Long> asyncIds = new ArrayList<>();
    public int ready;
    /** 0: answer batches, 1: throw from the batch method. */
    public int batchMode;
    public int singleMeasures;
    public int batchCalls;
    public int batchedTexts;
    /** Set once destroy has returned; any callback after that counts as late. */
    public volatile boolean destroyed;
    public volatile int late;
    /** Milliseconds each log callback sleeps (a slow host). */
    public volatile int slowLog;

    private void seen() {
        if (destroyed) late++;
    }

    static float[] metrics(String attributedJson) {
        return new float[] {8f * attributedJson.length() / 4f, 20f, 15f};
    }

    @Override public synchronized float[] measureText(String attributedJson, float width, int widthMode) {
        seen();
        singleMeasures++;
        return metrics(attributedJson);
    }

    @Override public synchronized float[] measureTextBatch(String[] attributedJson, float[] widths, int[] widthModes) {
        seen();
        batchCalls++;
        batchedTexts += attributedJson.length;
        if (batchMode == 1) throw new IllegalStateException("batch measurer broke");
        float[] out = new float[attributedJson.length * 3];
        for (int i = 0; i < attributedJson.length; i++) System.arraycopy(metrics(attributedJson[i]), 0, out, i * 3, 3);
        return out;
    }

    public int lateCount() { return late; }

    public synchronized int counter(String name) {
        switch (name) {
            case "single": return singleMeasures;
            case "calls": return batchCalls;
            default: return batchedTexts;
        }
    }

    @Override public float[] measureComponent(String name, String propsJson, float width, int widthMode) {
        seen();
        return null;
    }

    @Override public synchronized void batchReady() { seen(); ready++; }

    @Override public String callSync(String module, String method, String argsJson) {
        seen();
        switch (method) {
            // Non-BMP characters cross as surrogate pairs in Java and must arrive as UTF-8.
            case "greet": return "\"hello 😀 " + argsJson.replaceAll("[\\[\\]\"]", "") + "\"";
            case "fail": throw new IllegalStateException("no such user");
            default: return "null";
        }
    }

    @Override public synchronized void callAsync(long requestId, String module, String method, String argsJson) {
        seen();
        asyncIds.add(requestId);
        asyncCalls.add(module + "." + method + argsJson);
    }

    @Override public synchronized void log(int level, String message) {
        seen();
        if (slowLog > 0) {
            try { Thread.sleep(slowLog); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
        }
        logs.add(level + ": " + message);
    }

    @Override public synchronized void error(String message) { seen(); errors.add(message); }

    public synchronized long firstAsyncId() { return asyncIds.isEmpty() ? -1 : asyncIds.get(0); }

    public synchronized String firstAsyncCall() { return asyncCalls.isEmpty() ? "" : asyncCalls.get(0); }

    public synchronized int errorCount() { return errors.size(); }

    public synchronized int readyCount() { return ready; }
}
