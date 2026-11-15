#include <CoreFoundation/CoreFoundation.h>
#include <CoreGraphics/CoreGraphics.h>
#include <mach/mach_time.h>
#include <math.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "shim.h"

static mach_timebase_info_data_t tb;

static double ticks_to_ns(uint64_t t) { return (double)t * tb.numer / tb.denom; }

static int cmp_double(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}

static void report(const char *name, double *v, size_t n) {
    if (n == 0) {
        printf("%s n=0\n", name);
        return;
    }
    qsort(v, n, sizeof(double), cmp_double);
    double sum = 0;
    for (size_t i = 0; i < n; ++i) sum += v[i];
    size_t i99 = (size_t)ceil(0.99 * (double)n) - 1;
    printf("%s n=%zu p50=%.1f p99=%.1f max=%.1f mean=%.1f\n", name, n, v[n / 2], v[i99], v[n - 1],
           sum / (double)n);
    fflush(stdout);
}

static void setup(void) {
    kb_init();
    kb_load_themes();
    usleep(200 * 1000);
    kb_stop_audio_unit();
}

static const int kExplicit[] = {49, 36, 48, 51, 56, 59};
static const int kFallback[] = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12};
#define LEN(a) (sizeof(a) / sizeof((a)[0]))

static uint64_t g_seq;

static void mode_startup(void) {
    kb_init();
    uint64_t t0 = mach_absolute_time();
    kb_load_themes();
    uint64_t t1 = mach_absolute_time();
    double v = ticks_to_ns(t1 - t0);
    report("startup_load_themes", &v, 1);
}

static void bench_play(const char *name, const int *keys, size_t nkeys) {
    enum { N = 200000, B = 32, NB = 20000 };
    double *single = malloc(sizeof(double) * N);
    double *batch = malloc(sizeof(double) * NB);
    for (size_t i = 0; i < N; ++i) {
        int key = keys[i % nkeys];
        uint64_t t0 = mach_absolute_time();
        kb_play(key, (int)(i & 1), 0);
        uint64_t t1 = mach_absolute_time();
        kb_ack(g_seq++);
        single[i] = ticks_to_ns(t1 - t0);
    }
    for (size_t b = 0; b < NB; ++b) {
        uint64_t t0 = mach_absolute_time();
        for (size_t j = 0; j < B; ++j) kb_play(keys[(b * B + j) % nkeys], (int)(j & 1), 0);
        uint64_t t1 = mach_absolute_time();
        for (size_t j = 0; j < B; ++j) kb_ack(g_seq++);
        batch[b] = ticks_to_ns(t1 - t0) / B;
    }
    char label[128];
    snprintf(label, sizeof label, "%s_single", name);
    report(label, single, N);
    snprintf(label, sizeof label, "%s_batch32_per_call", name);
    report(label, batch, NB);
    free(single);
    free(batch);
}

static void mode_play(void) {
    setup();
    bench_play("play_explicit", kExplicit, LEN(kExplicit));
    bench_play("play_fallback", kFallback, LEN(kFallback));
}

static CGEventRef make_key(int key, bool down) {
    CGEventRef e = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key, down);
    CGEventSetFlags(e, 0);
    return e;
}

static CGEventRef make_flags(int key, CGEventFlags flags) {
    CGEventRef e = CGEventCreateKeyboardEvent(NULL, (CGKeyCode)key, true);
    CGEventSetType(e, kCGEventFlagsChanged);
    CGEventSetFlags(e, flags);
    return e;
}

static void report_logged(const char *name, const char *path) {
    FILE *f = fopen(path, "r");
    if (!f) return;
    size_t cap = 1 << 16, n = 0;
    double *v = malloc(sizeof(double) * cap);
    char line[512];
    while (fgets(line, sizeof line, f)) {
        char *p = strstr(line, "software=");
        if (!p) continue;
        if (n == cap) v = realloc(v, sizeof(double) * (cap *= 2));
        v[n++] = strtod(p + 9, NULL) * 1000.0;
    }
    fclose(f);
    report(name, v, n);
    free(v);
}

static void mode_callback(const char *logpath) {
    setup();
    if (!freopen(logpath, "w", stderr)) return;
    enum { N = 30000 };
    CGEventRef events[6] = {
        make_key(49, true), make_key(49, false),
        make_key(0, true),  make_key(0, false),
        make_flags(56, kCGEventFlagMaskShift), make_flags(56, 0),
    };
    CGEventType types[6] = {kCGEventKeyDown, kCGEventKeyUp, kCGEventKeyDown,
                            kCGEventKeyUp,   kCGEventFlagsChanged, kCGEventFlagsChanged};
    double *v = malloc(sizeof(double) * N);
    for (size_t i = 0; i < N; ++i) {
        uint64_t t0 = mach_absolute_time();
        kb_callback(types[i % 6], events[i % 6]);
        uint64_t t1 = mach_absolute_time();
        kb_ack(g_seq++);
        v[i] = ticks_to_ns(t1 - t0);
    }
    fflush(stderr);
    report("callback_total_incl_log_write", v, N);
    report_logged("callback_logged_entry_to_publish", logpath);
    free(v);
}

static void mode_render(void) {
    setup();
    enum { FRAMES = 128, TIMED = 300, TOTAL = 350, REPS = 67 };
    float left[FRAMES], right[FRAMES];
    static const int counts[] = {0, 1, 8, 16};
    double *v = malloc(sizeof(double) * TIMED * REPS);
    for (size_t c = 0; c < LEN(counts); ++c) {
        size_t n = 0;
        for (int rep = 0; rep < REPS; ++rep) {
            for (int k = 0; k < counts[c]; ++k) kb_play(kFallback[k % LEN(kFallback)], 0, 0);
            for (int b = 0; b < TOTAL; ++b) {
                uint64_t t0 = mach_absolute_time();
                kb_render(FRAMES, left, right);
                uint64_t t1 = mach_absolute_time();
                if (b < TIMED) v[n++] = ticks_to_ns(t1 - t0);
            }
        }
        char label[64];
        snprintf(label, sizeof label, "render_128f_%dvoices", counts[c]);
        report(label, v, n);
    }
    free(v);
}

static void mode_golden(const char *outpath) {
    setup();
    FILE *out = fopen(outpath, "wb");
    if (!out) return;
    float left[512], right[512];
    static const uint32_t sizes[] = {128, 118, 119, 512, 1, 37};
    for (int b = 0; b < 1200; ++b) {
        if (b % 7 == 0) kb_play(kFallback[b % LEN(kFallback)], 0, 0);
        if (b % 11 == 0) kb_play(kExplicit[b % LEN(kExplicit)], 1, 0);
        if (b % 50 == 0)
            for (int k = 0; k < 20; ++k) kb_play(k % 3 ? kFallback[k % LEN(kFallback)] : 49, k & 1, 0);
        uint32_t frames = sizes[b % LEN(sizes)];
        kb_render(frames, left, right);
        fwrite(left, sizeof(float), frames, out);
        fwrite(right, sizeof(float), frames, out);
    }
    fclose(out);
    printf("golden written\n");
}

#define TAG 0x4b5245414d42ULL

static CFMachPortRef g_tail_tap;
static volatile long g_dropped;
static CFRunLoopRef g_main_loop;
static long g_tap_n;

static CGEventRef tail_cb(CGEventTapProxy proxy, CGEventType type, CGEventRef e, void *info) {
    (void)proxy, (void)info;
    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
        CGEventTapEnable(g_tail_tap, true);
        return e;
    }
    if ((uint64_t)CGEventGetIntegerValueField(e, kCGEventSourceUserData) == TAG) {
        __atomic_add_fetch(&g_dropped, 1, __ATOMIC_RELAXED);
        return NULL;
    }
    return e;
}

static void *poster(void *arg) {
    (void)arg;
    long n = g_tap_n;
    double *v = malloc(sizeof(double) * (size_t)n);
    size_t ok = 0, missed = 0;
    usleep(300 * 1000);
    for (long i = 0; i < n; ++i) {
        CGEventRef e = CGEventCreateKeyboardEvent(NULL, 80, (i & 1) == 0);
        CGEventSetFlags(e, 0);
        CGEventSetIntegerValueField(e, kCGEventSourceUserData, (int64_t)TAG);
        uint64_t deadline = mach_absolute_time() + (uint64_t)(500e6 * tb.denom / tb.numer);
        uint64_t t0 = mach_absolute_time();
        CGEventPost(kCGHIDEventTap, e);
        uint64_t t1;
        int seen = 0;
        while ((t1 = mach_absolute_time()) < deadline)
            if (kb_poll_published(g_seq)) {
                seen = 1;
                break;
            }
        if (seen) {
            v[ok++] = ticks_to_ns(t1 - t0);
            kb_ack(g_seq);
            g_seq++;
        } else {
            missed++;
        }
        CFRelease(e);
        usleep(3000);
    }
    usleep(100 * 1000);
    report("tap_post_to_publish", v, ok);
    printf("tap_missed n=%zu dropped_by_tail_tap=%ld\n", missed, g_dropped);
    free(v);
    CFRunLoopStop(g_main_loop);
    return NULL;
}

static int mode_tap(const char *logpath, long n) {
    setup();
    if (!freopen(logpath, "w", stderr)) return 1;
    g_tap_n = n;
    if (kb_input_start() != 0) {
        printf("tap_unavailable\n");
        return 2;
    }
    g_tail_tap = CGEventTapCreate(kCGSessionEventTap, kCGTailAppendEventTap, kCGEventTapOptionDefault,
                                  CGEventMaskBit(kCGEventKeyDown) | CGEventMaskBit(kCGEventKeyUp),
                                  tail_cb, NULL);
    if (!g_tail_tap) {
        printf("tap_unavailable (tail tap)\n");
        return 2;
    }
    CFRunLoopSourceRef src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, g_tail_tap, 0);
    g_main_loop = CFRunLoopGetCurrent();
    CFRunLoopAddSource(g_main_loop, src, kCFRunLoopCommonModes);
    CGEventTapEnable(g_tail_tap, true);
    pthread_t th;
    pthread_create(&th, NULL, poster, NULL);
    CFRunLoopRun();
    pthread_join(th, NULL);
    fflush(stderr);
    report_logged("tap_logged_entry_to_publish", logpath);
    return 0;
}

int main(int argc, char **argv) {
    mach_timebase_info(&tb);
    if (argc < 2) {
        fprintf(stderr, "usage: bench startup|play|callback LOG|render|golden OUT|tap LOG N\n");
        return 1;
    }
    const char *m = argv[1];
    if (!strcmp(m, "startup")) mode_startup();
    else if (!strcmp(m, "play")) mode_play();
    else if (!strcmp(m, "callback") && argc > 2) mode_callback(argv[2]);
    else if (!strcmp(m, "render")) mode_render();
    else if (!strcmp(m, "golden") && argc > 2) mode_golden(argv[2]);
    else if (!strcmp(m, "tap") && argc > 3) return mode_tap(argv[2], atol(argv[3]));
    else return 1;
    return 0;
}
