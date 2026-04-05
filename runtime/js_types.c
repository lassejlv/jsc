// runtime.c - JavaScript runtime for js-compiler
// NaN-boxing based dynamic type system
//
// Every JS value is a 64-bit integer. Numbers are stored as the raw bits
// of an IEEE 754 double. Non-number values are encoded in the NaN space.

#define _CRT_SECURE_NO_WARNINGS
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <math.h>
#include <setjmp.h>
#include <time.h>
#include <float.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <sys/time.h>
#include <unistd.h>
#define _strdup strdup
#endif

#include <curl/curl.h>
#include <regex.h>

// ============================================================
// NaN-boxing value representation
// ============================================================

typedef int64_t JSValue;

// Forward declarations
void js_this_push(JSValue v);
void js_this_pop(void);
JSValue js_this_get(void);
JSValue js_array_sort(JSValue arr_val, JSValue compare_fn);
JSValue js_array_splice(JSValue arr_val, JSValue* args, int argc);
JSValue js_json_parse(JSValue str);
JSValue js_fetch(JSValue url_val, JSValue options_val);
JSValue js_call_func_spread(JSValue func_val, JSValue arr_val);
// Promise
JSValue js_promise_create(JSValue executor);
JSValue js_promise_resolve_static(JSValue value);
JSValue js_promise_reject_static(JSValue reason);
JSValue js_promise_all(JSValue arr_val);
JSValue js_promise_race(JSValue arr_val);
JSValue js_promise_all_settled(JSValue arr_val);
JSValue js_await(JSValue value);
JSValue js_async_return(JSValue value);
JSValue js_async_throw(JSValue error);
JSValue js_promise_then(JSValue promise, JSValue on_fulfilled, JSValue on_rejected);
// Timers
JSValue js_set_timeout(JSValue callback, JSValue delay_val);
JSValue js_set_interval(JSValue callback, JSValue delay_val);
JSValue js_clear_timeout(JSValue id_val);
void js_run_event_loop(void);
// Web APIs
JSValue js_headers_new(JSValue init);
JSValue js_url_new(const char* url_str);
JSValue js_request_new(JSValue url_val, JSValue init_val);
JSValue js_response_new(JSValue body_val, JSValue init_val);
JSValue js_response_json(JSValue data, JSValue init_val);
JSValue js_response_redirect(JSValue url_val, JSValue status_val);
JSValue js_url_new_val(JSValue url_val);
// Server
JSValue js_serve(JSValue config);
// Map / Set / RegExp
JSValue js_map_new(void);
JSValue js_map_set(JSValue map_val, JSValue key, JSValue value);
JSValue js_map_get(JSValue map_val, JSValue key);
JSValue js_map_has(JSValue map_val, JSValue key);
JSValue js_map_delete(JSValue map_val, JSValue key);
JSValue js_set_new(void);
JSValue js_regexp_new(JSValue pattern_val, JSValue flags_val);
JSValue js_regexp_test(JSValue re_val, JSValue str_val);
JSValue js_regexp_exec(JSValue re_val, JSValue str_val);
typedef struct { JSValue* keys; JSValue* values; int length; int capacity; } JSMapData;
JSMapData* map_get_data(JSValue map_val);
int map_find_key(JSMapData* data, JSValue key);

#define QNAN        ((uint64_t)0x7FFC000000000000ULL)
#define SIGN_BIT    ((uint64_t)0x8000000000000000ULL)

#define BOOL_TAG    (QNAN | 0x0001000000000000ULL)
#define NULL_TAG    (QNAN | 0x0002000000000000ULL)
#define UNDEF_TAG   (QNAN | 0x0003000000000000ULL)
#define STRING_TAG  (QNAN | SIGN_BIT)
#define OBJECT_TAG  (QNAN | SIGN_BIT | 0x0001000000000000ULL)
#define ARRAY_TAG   (QNAN | SIGN_BIT | 0x0002000000000000ULL)
#define FUNC_TAG    (QNAN | SIGN_BIT | 0x0003000000000000ULL)

#define TAG_MASK    (QNAN | SIGN_BIT | 0x0007000000000000ULL)
#define PTR_MASK    (0x0000FFFFFFFFFFFFULL)

#define JS_TRUE     ((JSValue)(BOOL_TAG | 1))
#define JS_FALSE    ((JSValue)(BOOL_TAG))
#define JS_NULL     ((JSValue)NULL_TAG)
#define JS_UNDEFINED ((JSValue)UNDEF_TAG)

static inline int js_is_number(JSValue v)    { return ((uint64_t)v & QNAN) != QNAN; }
static inline int js_is_bool(JSValue v)      { return ((uint64_t)v & TAG_MASK) == BOOL_TAG; }
static inline int js_is_null(JSValue v)      { return (uint64_t)v == NULL_TAG; }
static inline int js_is_undefined(JSValue v) { return (uint64_t)v == UNDEF_TAG; }
static inline int js_is_string(JSValue v)    { return ((uint64_t)v & TAG_MASK) == STRING_TAG; }
static inline int js_is_object(JSValue v)    { return ((uint64_t)v & TAG_MASK) == OBJECT_TAG; }
static inline int js_is_array(JSValue v)     { return ((uint64_t)v & TAG_MASK) == ARRAY_TAG; }
static inline int js_is_function(JSValue v)  { return ((uint64_t)v & TAG_MASK) == FUNC_TAG; }

static inline JSValue js_number(double n) {
    JSValue v; memcpy(&v, &n, sizeof(double)); return v;
}
static inline double js_as_number(JSValue v) {
    double n; memcpy(&n, &v, sizeof(double)); return n;
}
static inline int js_as_bool(JSValue v)  { return (int)((uint64_t)v & 1); }
static inline void* js_as_ptr(JSValue v) { return (void*)((uint64_t)v & PTR_MASK); }

// ============================================================
// Heap types
// ============================================================

typedef enum { HEAP_STRING, HEAP_OBJECT, HEAP_ARRAY, HEAP_FUNCTION } HeapType;

typedef struct { HeapType type; int refcount; } HeapHeader;

typedef struct {
    HeapHeader header;
    int length;
    char* data; // null-terminated
} JSString;

typedef struct {
    char* key;
    JSValue value;
    int occupied; // 0=empty, 1=occupied, 2=tombstone
} ObjectEntry;

typedef struct {
    HeapHeader header;
    int capacity;
    int count;
    ObjectEntry* entries;
} JSObject;

typedef struct {
    HeapHeader header;
    int capacity;
    int length;
    JSValue* data;
} JSArray;

typedef JSValue (*JSNativeFunc)(JSValue* args, int argc, void* closure);

typedef struct {
    HeapHeader header;
    JSNativeFunc fn;
    void* closure_env;
    int arity;
} JSFunction;

// Forward declarations
char* js_to_cstring(JSValue v);
JSValue js_to_string_val(JSValue v);
int js_is_truthy(JSValue v);
double js_to_number(JSValue v);
void js_release(JSValue v);
JSValue js_json_stringify(JSValue v);
JSValue js_object_rest(JSValue src, JSValue exclude_keys);
JSValue js_bind_trampoline(JSValue* args, int argc, void* closure);
void* js_alloc_closure_env(int size);
JSValue js_func_new(JSNativeFunc fn, void* closure_env, int arity);

