// ============================================================
// Promise implementation
// ============================================================

#define PROMISE_PENDING   0
#define PROMISE_FULFILLED 1
#define PROMISE_REJECTED  2

static int js_is_promise(JSValue v) {
    if (!js_is_object(v)) return 0;
    JSValue type = js_object_get((JSObject*)js_as_ptr(v), "__type");
    if (!js_is_string(type)) return 0;
    return strcmp(js_as_string(type)->data, "Promise") == 0;
}

static JSValue js_promise_new(void) {
    JSValue p = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(p);
    js_object_set(obj, "__type", js_string_from_cstr("Promise"));
    js_object_set(obj, "__state", js_number(PROMISE_PENDING));
    js_object_set(obj, "__value", JS_UNDEFINED);
    js_object_set(obj, "__callbacks", js_array_new());
    return p;
}

static void js_promise_run_callbacks(JSValue promise);

static void js_promise_resolve(JSValue promise, JSValue value) {
    JSObject* obj = (JSObject*)js_as_ptr(promise);
    double state = js_as_number(js_object_get(obj, "__state"));
    if (state != PROMISE_PENDING) return;

    // If value is a Promise, adopt its state
    if (js_is_promise(value)) {
        JSObject* inner = (JSObject*)js_as_ptr(value);
        double inner_state = js_as_number(js_object_get(inner, "__state"));
        if (inner_state == PROMISE_PENDING) {
            // Chain: when inner settles, settle this promise
            // For our sync runtime, this shouldn't happen, but handle it
            JSValue inner_val = js_object_get(inner, "__value");
            js_object_set(obj, "__state", js_number(PROMISE_FULFILLED));
            js_object_set(obj, "__value", inner_val);
        } else {
            js_object_set(obj, "__state", js_object_get(inner, "__state"));
            js_object_set(obj, "__value", js_object_get(inner, "__value"));
        }
    } else {
        js_object_set(obj, "__state", js_number(PROMISE_FULFILLED));
        js_object_set(obj, "__value", value);
    }
    js_promise_run_callbacks(promise);
}

static void js_promise_reject(JSValue promise, JSValue reason) {
    JSObject* obj = (JSObject*)js_as_ptr(promise);
    double state = js_as_number(js_object_get(obj, "__state"));
    if (state != PROMISE_PENDING) return;

    js_object_set(obj, "__state", js_number(PROMISE_REJECTED));
    js_object_set(obj, "__value", reason);
    js_promise_run_callbacks(promise);
}

static void js_promise_run_callbacks(JSValue promise) {
    JSObject* obj = (JSObject*)js_as_ptr(promise);
    int state = (int)js_as_number(js_object_get(obj, "__state"));
    JSValue value = js_object_get(obj, "__value");
    JSValue callbacks = js_object_get(obj, "__callbacks");

    if (!js_is_array(callbacks)) return;
    JSArray* cbs = (JSArray*)js_as_ptr(callbacks);

    for (int i = 0; i < cbs->length; i++) {
        JSValue entry = cbs->data[i];
        if (!js_is_array(entry)) continue;
        JSArray* e = (JSArray*)js_as_ptr(entry);
        if (e->length < 3) continue;

        JSValue handler = (state == PROMISE_FULFILLED) ? e->data[0] : e->data[1];
        JSValue next_promise = e->data[2];

        if (js_is_function(handler)) {
            JSFunction* fn = (JSFunction*)js_as_ptr(handler);
            JSValue args[1] = { value };

            // try/catch around handler
            void* buf = js_try_get_buf();
            if (_setjmp(buf) == 0) {
                JSValue result = fn->fn(args, 1, fn->closure_env);
                js_try_exit();
                js_promise_resolve(next_promise, result);
            } else {
                JSValue err = js_get_error();
                js_promise_reject(next_promise, err);
            }
        } else {
            if (state == PROMISE_FULFILLED)
                js_promise_resolve(next_promise, value);
            else
                js_promise_reject(next_promise, value);
        }
    }

    // Clear callbacks
    js_object_set(obj, "__callbacks", js_array_new());
}

// .then(onFulfilled, onRejected) -> Promise
JSValue js_promise_then(JSValue promise, JSValue on_fulfilled, JSValue on_rejected) {
    JSValue next = js_promise_new();
    JSObject* obj = (JSObject*)js_as_ptr(promise);
    int state = (int)js_as_number(js_object_get(obj, "__state"));

    if (state == PROMISE_PENDING) {
        JSValue callbacks = js_object_get(obj, "__callbacks");
        JSValue entry = js_array_new();
        js_array_push_val(entry, on_fulfilled);
        js_array_push_val(entry, on_rejected);
        js_array_push_val(entry, next);
        js_array_push_val(callbacks, entry);
    } else {
        JSValue value = js_object_get(obj, "__value");
        JSValue handler = (state == PROMISE_FULFILLED) ? on_fulfilled : on_rejected;

        if (js_is_function(handler)) {
            JSFunction* fn = (JSFunction*)js_as_ptr(handler);
            JSValue args[1] = { value };
            void* buf = js_try_get_buf();
            if (_setjmp(buf) == 0) {
                JSValue result = fn->fn(args, 1, fn->closure_env);
                js_try_exit();
                js_promise_resolve(next, result);
            } else {
                JSValue err = js_get_error();
                js_promise_reject(next, err);
            }
        } else {
            if (state == PROMISE_FULFILLED)
                js_promise_resolve(next, value);
            else
                js_promise_reject(next, value);
        }
    }

    return next;
}

// Executor-based resolve/reject closures
static JSValue promise_resolve_wrapper(JSValue* args, int argc, void* closure) {
    JSValue promise = ((JSValue*)closure)[0];
    JSValue value = argc > 0 ? args[0] : JS_UNDEFINED;
    js_promise_resolve(promise, value);
    return JS_UNDEFINED;
}

static JSValue promise_reject_wrapper(JSValue* args, int argc, void* closure) {
    JSValue promise = ((JSValue*)closure)[0];
    JSValue reason = argc > 0 ? args[0] : JS_UNDEFINED;
    js_promise_reject(promise, reason);
    return JS_UNDEFINED;
}

// new Promise(executor)
JSValue js_promise_create(JSValue executor) {
    JSValue promise = js_promise_new();

    if (!js_is_function(executor)) return promise;

    // Create resolve/reject with promise captured in closure env
    JSValue* resolve_env = (JSValue*)malloc(sizeof(JSValue));
    resolve_env[0] = promise;
    JSValue* reject_env = (JSValue*)malloc(sizeof(JSValue));
    reject_env[0] = promise;

    JSValue resolve = js_func_new((JSNativeFunc)promise_resolve_wrapper, resolve_env, 1);
    JSValue reject = js_func_new((JSNativeFunc)promise_reject_wrapper, reject_env, 1);

    JSFunction* fn = (JSFunction*)js_as_ptr(executor);
    JSValue exec_args[2] = { resolve, reject };

    void* buf = js_try_get_buf();
    if (_setjmp(buf) == 0) {
        fn->fn(exec_args, 2, fn->closure_env);
        js_try_exit();
    } else {
        JSValue err = js_get_error();
        js_promise_reject(promise, err);
    }

    return promise;
}

// Promise.resolve(value)
JSValue js_promise_resolve_static(JSValue value) {
    if (js_is_promise(value)) return value;
    JSValue p = js_promise_new();
    js_promise_resolve(p, value);
    return p;
}

// Promise.reject(reason)
JSValue js_promise_reject_static(JSValue reason) {
    JSValue p = js_promise_new();
    js_promise_reject(p, reason);
    return p;
}

// Promise.all(iterable)
JSValue js_promise_all(JSValue arr_val) {
    if (!js_is_array(arr_val)) return js_promise_resolve_static(js_array_new());
    JSArray* arr = (JSArray*)js_as_ptr(arr_val);
    JSValue results = js_array_new();

    for (int i = 0; i < arr->length; i++) {
        JSValue item = arr->data[i];
        if (js_is_promise(item)) {
            JSObject* p = (JSObject*)js_as_ptr(item);
            int state = (int)js_as_number(js_object_get(p, "__state"));
            JSValue val = js_object_get(p, "__value");
            if (state == PROMISE_REJECTED) {
                return js_promise_reject_static(val);
            }
            js_array_push_val(results, val);
        } else {
            js_array_push_val(results, item);
        }
    }
    return js_promise_resolve_static(results);
}

// Promise.race(iterable)
JSValue js_promise_race(JSValue arr_val) {
    if (!js_is_array(arr_val)) return js_promise_resolve_static(JS_UNDEFINED);
    JSArray* arr = (JSArray*)js_as_ptr(arr_val);

    for (int i = 0; i < arr->length; i++) {
        JSValue item = arr->data[i];
        if (js_is_promise(item)) {
            JSObject* p = (JSObject*)js_as_ptr(item);
            int state = (int)js_as_number(js_object_get(p, "__state"));
            if (state != PROMISE_PENDING) {
                JSValue val = js_object_get(p, "__value");
                if (state == PROMISE_FULFILLED) return js_promise_resolve_static(val);
                else return js_promise_reject_static(val);
            }
        } else {
            return js_promise_resolve_static(item);
        }
    }
    return js_promise_new(); // all pending — return pending promise
}

// Promise.allSettled(iterable)
JSValue js_promise_all_settled(JSValue arr_val) {
    if (!js_is_array(arr_val)) return js_promise_resolve_static(js_array_new());
    JSArray* arr = (JSArray*)js_as_ptr(arr_val);
    JSValue results = js_array_new();

    for (int i = 0; i < arr->length; i++) {
        JSValue item = arr->data[i];
        JSValue entry = js_object_new();
        JSObject* e = (JSObject*)js_as_ptr(entry);
        if (js_is_promise(item)) {
            JSObject* p = (JSObject*)js_as_ptr(item);
            int state = (int)js_as_number(js_object_get(p, "__state"));
            JSValue val = js_object_get(p, "__value");
            if (state == PROMISE_FULFILLED) {
                js_object_set(e, "status", js_string_from_cstr("fulfilled"));
                js_object_set(e, "value", val);
            } else {
                js_object_set(e, "status", js_string_from_cstr("rejected"));
                js_object_set(e, "reason", val);
            }
        } else {
            js_object_set(e, "status", js_string_from_cstr("fulfilled"));
            js_object_set(e, "value", item);
        }
        js_array_push_val(results, entry);
    }
    return js_promise_resolve_static(results);
}

// await: extract value from settled Promise, or return non-Promise
JSValue js_await(JSValue value) {
    if (!js_is_promise(value)) return value;

    JSObject* obj = (JSObject*)js_as_ptr(value);
    int state = (int)js_as_number(js_object_get(obj, "__state"));
    JSValue val = js_object_get(obj, "__value");

    if (state == PROMISE_REJECTED) {
        js_throw(val);
        return JS_UNDEFINED;
    }
    return val; // fulfilled or pending (pending returns undefined in sync model)
}

// Wrap a return value from an async function into a resolved Promise
JSValue js_async_return(JSValue value) {
    return js_promise_resolve_static(value);
}

// Wrap a thrown error from an async function into a rejected Promise
JSValue js_async_throw(JSValue error) {
    return js_promise_reject_static(error);
}

// ============================================================
// setTimeout / setInterval / clearTimeout / clearInterval
// ============================================================

typedef struct {
    JSValue callback;
    double fire_at_ms;
    double interval_ms; // 0 for setTimeout
    int id;
    int active;
} TimerEntry;

#define MAX_TIMERS 256
static TimerEntry js_timers[MAX_TIMERS];
static int js_timer_count = 0;
static int js_next_timer_id = 1;

static double js_now_ms(void) {
#ifdef _WIN32
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    uint64_t t = ((uint64_t)ft.dwHighDateTime << 32) | ft.dwLowDateTime;
    t -= 116444736000000000ULL;
    t /= 10000;
    return (double)t;
#else
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return (double)tv.tv_sec * 1000.0 + (double)tv.tv_usec / 1000.0;
#endif
}

JSValue js_set_timeout(JSValue callback, JSValue delay_val) {
    if (!js_is_function(callback) || js_timer_count >= MAX_TIMERS) return JS_UNDEFINED;
    double delay_ms = js_is_number(delay_val) ? js_as_number(delay_val) : 0;
    if (delay_ms < 0) delay_ms = 0;

    int id = js_next_timer_id++;
    js_timers[js_timer_count].callback = callback;
    js_timers[js_timer_count].fire_at_ms = js_now_ms() + delay_ms;
    js_timers[js_timer_count].interval_ms = 0;
    js_timers[js_timer_count].id = id;
    js_timers[js_timer_count].active = 1;
    js_timer_count++;
    return js_number((double)id);
}

JSValue js_set_interval(JSValue callback, JSValue delay_val) {
    if (!js_is_function(callback) || js_timer_count >= MAX_TIMERS) return JS_UNDEFINED;
    double delay_ms = js_is_number(delay_val) ? js_as_number(delay_val) : 0;
    if (delay_ms < 1) delay_ms = 1;

    int id = js_next_timer_id++;
    js_timers[js_timer_count].callback = callback;
    js_timers[js_timer_count].fire_at_ms = js_now_ms() + delay_ms;
    js_timers[js_timer_count].interval_ms = delay_ms;
    js_timers[js_timer_count].id = id;
    js_timers[js_timer_count].active = 1;
    js_timer_count++;
    return js_number((double)id);
}

JSValue js_clear_timeout(JSValue id_val) {
    if (!js_is_number(id_val)) return JS_UNDEFINED;
    int id = (int)js_as_number(id_val);
    for (int i = 0; i < js_timer_count; i++) {
        if (js_timers[i].id == id) {
            js_timers[i].active = 0;
            break;
        }
    }
    return JS_UNDEFINED;
}

// Event loop: process timers in time order
void js_run_event_loop(void) {
    while (1) {
        // Find the earliest active timer
        int earliest = -1;
        double earliest_time = 1e18;
        int any_active = 0;

        for (int i = 0; i < js_timer_count; i++) {
            if (!js_timers[i].active) continue;
            any_active = 1;
            if (js_timers[i].fire_at_ms < earliest_time) {
                earliest_time = js_timers[i].fire_at_ms;
                earliest = i;
            }
        }

        if (!any_active) break;

        // Wait for it
        double now = js_now_ms();
        if (earliest_time > now) {
            int wait_us = (int)((earliest_time - now) * 1000.0);
            if (wait_us > 0) {
#ifdef _WIN32
                Sleep(wait_us / 1000);
#else
                usleep(wait_us);
#endif
            }
        }

        // Fire it
        if (earliest >= 0 && js_timers[earliest].active) {
            JSFunction* fn = (JSFunction*)js_as_ptr(js_timers[earliest].callback);
            fn->fn(NULL, 0, fn->closure_env);

            if (js_timers[earliest].interval_ms > 0) {
                js_timers[earliest].fire_at_ms = js_now_ms() + js_timers[earliest].interval_ms;
            } else {
                js_timers[earliest].active = 0;
            }
        }
    }
}
