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
#define _strdup strdup
#endif

// ============================================================
// NaN-boxing value representation
// ============================================================

typedef int64_t JSValue;

// Forward declarations
void js_this_push(JSValue v);
void js_this_pop(void);
JSValue js_this_get(void);

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

// ============================================================
// String operations
// ============================================================

static JSString* js_string_alloc(const char* data, int len) {
    JSString* s = (JSString*)malloc(sizeof(JSString));
    s->header.type = HEAP_STRING;
    s->header.refcount = 1;
    s->length = len;
    s->data = (char*)malloc(len + 1);
    if (data) memcpy(s->data, data, len);
    s->data[len] = '\0';
    return s;
}

JSValue js_string_from_cstr(const char* str) {
    int len = (int)strlen(str);
    JSString* s = js_string_alloc(str, len);
    return (JSValue)(STRING_TAG | ((uint64_t)s & PTR_MASK));
}

JSValue js_string_from_len(const char* str, int len) {
    JSString* s = js_string_alloc(str, len);
    return (JSValue)(STRING_TAG | ((uint64_t)s & PTR_MASK));
}

static JSString* js_as_string(JSValue v) { return (JSString*)js_as_ptr(v); }

static JSValue js_string_concat(JSValue a, JSValue b) {
    JSString* sa = js_as_string(a);
    JSString* sb = js_as_string(b);
    int new_len = sa->length + sb->length;
    JSString* r = js_string_alloc(NULL, new_len);
    memcpy(r->data, sa->data, sa->length);
    memcpy(r->data + sa->length, sb->data, sb->length);
    r->data[new_len] = '\0';
    return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
}

static int js_string_equals(JSString* a, JSString* b) {
    if (a->length != b->length) return 0;
    return memcmp(a->data, b->data, a->length) == 0;
}

static int js_string_compare(JSString* a, JSString* b) {
    int min = a->length < b->length ? a->length : b->length;
    int cmp = memcmp(a->data, b->data, min);
    if (cmp != 0) return cmp;
    return a->length - b->length;
}

// ============================================================
// Object operations (hash map)
// ============================================================

static unsigned int hash_string(const char* key) {
    unsigned int h = 2166136261u;
    while (*key) { h ^= (unsigned char)*key++; h *= 16777619u; }
    return h;
}

JSValue js_object_new(void) {
    JSObject* obj = (JSObject*)malloc(sizeof(JSObject));
    obj->header.type = HEAP_OBJECT;
    obj->header.refcount = 1;
    obj->capacity = 8;
    obj->count = 0;
    obj->entries = (ObjectEntry*)calloc(8, sizeof(ObjectEntry));
    return (JSValue)(OBJECT_TAG | ((uint64_t)obj & PTR_MASK));
}

static void js_object_grow(JSObject* obj) {
    int new_cap = obj->capacity * 2;
    ObjectEntry* ne = (ObjectEntry*)calloc(new_cap, sizeof(ObjectEntry));
    for (int i = 0; i < obj->capacity; i++) {
        if (obj->entries[i].occupied == 1) {
            unsigned int idx = hash_string(obj->entries[i].key) & (new_cap - 1);
            while (ne[idx].occupied == 1) idx = (idx + 1) & (new_cap - 1);
            ne[idx] = obj->entries[i];
        }
    }
    free(obj->entries);
    obj->entries = ne;
    obj->capacity = new_cap;
}

void js_object_set(JSObject* obj, const char* key, JSValue value) {
    if (obj->count * 3 >= obj->capacity * 2) js_object_grow(obj);
    unsigned int idx = hash_string(key) & (obj->capacity - 1);
    while (obj->entries[idx].occupied == 1) {
        if (strcmp(obj->entries[idx].key, key) == 0) {
            obj->entries[idx].value = value;
            return;
        }
        idx = (idx + 1) & (obj->capacity - 1);
    }
    obj->entries[idx].key = _strdup(key);
    obj->entries[idx].value = value;
    obj->entries[idx].occupied = 1;
    obj->count++;
}

JSValue js_object_get(JSObject* obj, const char* key) {
    unsigned int idx = hash_string(key) & (obj->capacity - 1);
    unsigned int start = idx;
    while (obj->entries[idx].occupied) {
        if (obj->entries[idx].occupied == 1 && strcmp(obj->entries[idx].key, key) == 0)
            return obj->entries[idx].value;
        idx = (idx + 1) & (obj->capacity - 1);
        if (idx == start) break;
    }
    return JS_UNDEFINED;
}

// ============================================================
// Array operations
// ============================================================

JSValue js_array_new(void) {
    JSArray* arr = (JSArray*)malloc(sizeof(JSArray));
    arr->header.type = HEAP_ARRAY;
    arr->header.refcount = 1;
    arr->capacity = 8;
    arr->length = 0;
    arr->data = (JSValue*)malloc(8 * sizeof(JSValue));
    return (JSValue)(ARRAY_TAG | ((uint64_t)arr & PTR_MASK));
}

static void js_array_grow(JSArray* arr) {
    arr->capacity *= 2;
    arr->data = (JSValue*)realloc(arr->data, arr->capacity * sizeof(JSValue));
}

JSValue js_array_push_val(JSValue av, JSValue elem) {
    JSArray* arr = (JSArray*)js_as_ptr(av);
    if (arr->length >= arr->capacity) js_array_grow(arr);
    arr->data[arr->length++] = elem;
    return js_number((double)arr->length);
}

JSValue js_array_pop_val(JSValue av) {
    JSArray* arr = (JSArray*)js_as_ptr(av);
    if (arr->length == 0) return JS_UNDEFINED;
    return arr->data[--arr->length];
}

JSValue js_array_get_index(JSValue av, int index) {
    JSArray* arr = (JSArray*)js_as_ptr(av);
    if (index < 0 || index >= arr->length) return JS_UNDEFINED;
    return arr->data[index];
}

void js_array_set_index(JSValue av, int index, JSValue value) {
    JSArray* arr = (JSArray*)js_as_ptr(av);
    while (index >= arr->capacity) js_array_grow(arr);
    while (arr->length <= index) arr->data[arr->length++] = JS_UNDEFINED;
    arr->data[index] = value;
}

// ============================================================
// Reference counting
// ============================================================

void js_release(JSValue v) {
    if (!js_is_string(v) && !js_is_object(v) && !js_is_array(v) && !js_is_function(v)) return;
    HeapHeader* h = (HeapHeader*)js_as_ptr(v);
    if (--h->refcount <= 0) {
        if (js_is_string(v)) {
            JSString* s = (JSString*)h;
            free(s->data); free(s);
        } else if (js_is_object(v)) {
            JSObject* obj = (JSObject*)h;
            for (int i = 0; i < obj->capacity; i++) {
                if (obj->entries[i].occupied == 1) {
                    free(obj->entries[i].key);
                    js_release(obj->entries[i].value);
                }
            }
            free(obj->entries); free(obj);
        } else if (js_is_array(v)) {
            JSArray* arr = (JSArray*)h;
            for (int i = 0; i < arr->length; i++) js_release(arr->data[i]);
            free(arr->data); free(arr);
        } else if (js_is_function(v)) {
            free(h);
        }
    }
}

// ============================================================
// Type coercion
// ============================================================

double js_to_number(JSValue v) {
    if (js_is_number(v)) return js_as_number(v);
    if (js_is_bool(v)) return js_as_bool(v) ? 1.0 : 0.0;
    if (js_is_null(v)) return 0.0;
    if (js_is_undefined(v)) return NAN;
    if (js_is_string(v)) {
        JSString* s = js_as_string(v);
        if (s->length == 0) return 0.0;
        char* end;
        double r = strtod(s->data, &end);
        while (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r') end++;
        if (*end != '\0') return NAN;
        return r;
    }
    return NAN;
}

JSValue js_to_number_val(JSValue v) { return js_number(js_to_number(v)); }

int js_to_boolean(JSValue v) {
    if (js_is_number(v)) { double n = js_as_number(v); return n != 0.0 && !isnan(n); }
    if (js_is_bool(v)) return js_as_bool(v);
    if (js_is_null(v) || js_is_undefined(v)) return 0;
    if (js_is_string(v)) return js_as_string(v)->length > 0;
    return 1; // objects, arrays, functions are truthy
}

int js_is_truthy(JSValue v) { return js_to_boolean(v); }

// Number to string with shortest representation (matches JS behavior)
static char* js_number_to_cstr(double n) {
    char buf[64];
    if (isnan(n)) return _strdup("NaN");
    if (isinf(n)) return _strdup(n > 0 ? "Infinity" : "-Infinity");
    if (n == 0.0) return _strdup("0");
    // Integer: no decimal point, no scientific notation for reasonable values
    if (n == floor(n) && fabs(n) < 1e20 && fabs(n) >= 1.0) {
        snprintf(buf, sizeof(buf), "%.0f", n);
        return _strdup(buf);
    }
    // Float: try increasing precision until roundtrip-safe
    for (int prec = 1; prec <= 21; prec++) {
        snprintf(buf, sizeof(buf), "%.*g", prec, n);
        if (strtod(buf, NULL) == n) return _strdup(buf);
    }
    snprintf(buf, sizeof(buf), "%.21g", n);
    return _strdup(buf);
}

char* js_to_cstring(JSValue v) {
    if (js_is_string(v)) return _strdup(js_as_string(v)->data);
    if (js_is_number(v)) return js_number_to_cstr(js_as_number(v));
    if (js_is_bool(v)) return _strdup(js_as_bool(v) ? "true" : "false");
    if (js_is_null(v)) return _strdup("null");
    if (js_is_undefined(v)) return _strdup("undefined");
    if (js_is_array(v)) {
        JSArray* arr = (JSArray*)js_as_ptr(v);
        if (arr->length == 0) return _strdup("");
        char** parts = (char**)malloc(arr->length * sizeof(char*));
        int total = 0;
        for (int i = 0; i < arr->length; i++) {
            parts[i] = js_to_cstring(arr->data[i]);
            total += (int)strlen(parts[i]);
        }
        total += arr->length - 1;
        char* r = (char*)malloc(total + 1);
        r[0] = '\0';
        for (int i = 0; i < arr->length; i++) {
            if (i > 0) strcat(r, ",");
            strcat(r, parts[i]);
            free(parts[i]);
        }
        free(parts);
        return r;
    }
    if (js_is_object(v)) return _strdup("[object Object]");
    if (js_is_function(v)) return _strdup("function() { [native code] }");
    return _strdup("undefined");
}

JSValue js_to_string_val(JSValue v) {
    if (js_is_string(v)) return v;
    char* s = js_to_cstring(v);
    JSValue r = js_string_from_cstr(s);
    free(s);
    return r;
}

// ============================================================
// Error handling
// ============================================================

#define MAX_TRY_DEPTH 64
static jmp_buf js_try_bufs[MAX_TRY_DEPTH];
static JSValue js_error_vals[MAX_TRY_DEPTH];
static int js_try_depth = 0;

void js_throw(JSValue error) {
    if (js_try_depth == 0) {
        fprintf(stderr, "Uncaught ");
        char* s = js_to_cstring(error);
        fprintf(stderr, "%s\n", s);
        free(s);
        exit(1);
    }
    js_try_depth--;
    js_error_vals[js_try_depth] = error;
    longjmp(js_try_bufs[js_try_depth], 1);
}

// js_try_enter: returns pointer to the jmp_buf to use, increments depth
void* js_try_get_buf(void) {
    if (js_try_depth >= MAX_TRY_DEPTH) {
        fprintf(stderr, "Error: try/catch nesting too deep\n");
        exit(1);
    }
    return &js_try_bufs[js_try_depth++];
}

void js_try_exit(void) {
    if (js_try_depth > 0) js_try_depth--;
}

JSValue js_get_error(void) {
    return js_error_vals[js_try_depth]; // depth was already decremented by longjmp path
}

JSValue js_error_new(const char* message) {
    JSValue obj = js_object_new();
    js_object_set((JSObject*)js_as_ptr(obj), "message", js_string_from_cstr(message));
    js_object_set((JSObject*)js_as_ptr(obj), "name", js_string_from_cstr("Error"));
    return obj;
}

// ============================================================
// Arithmetic operations
// ============================================================

JSValue js_add(JSValue a, JSValue b) {
    if (js_is_string(a) || js_is_string(b)) {
        JSValue sa = js_to_string_val(a);
        JSValue sb = js_to_string_val(b);
        JSValue r = js_string_concat(sa, sb);
        return r;
    }
    return js_number(js_to_number(a) + js_to_number(b));
}

JSValue js_sub(JSValue a, JSValue b) { return js_number(js_to_number(a) - js_to_number(b)); }
JSValue js_mul(JSValue a, JSValue b) { return js_number(js_to_number(a) * js_to_number(b)); }
JSValue js_div(JSValue a, JSValue b) { return js_number(js_to_number(a) / js_to_number(b)); }
JSValue js_mod(JSValue a, JSValue b) { return js_number(fmod(js_to_number(a), js_to_number(b))); }
JSValue js_neg(JSValue a)            { return js_number(-js_to_number(a)); }
JSValue js_not(JSValue a)            { return js_is_truthy(a) ? JS_FALSE : JS_TRUE; }
JSValue js_typeof_val(JSValue v) {
    if (js_is_number(v))    return js_string_from_cstr("number");
    if (js_is_bool(v))      return js_string_from_cstr("boolean");
    if (js_is_null(v))      return js_string_from_cstr("object"); // JS quirk
    if (js_is_undefined(v)) return js_string_from_cstr("undefined");
    if (js_is_string(v))    return js_string_from_cstr("string");
    if (js_is_function(v))  return js_string_from_cstr("function");
    return js_string_from_cstr("object");
}

// ============================================================
// Comparison operations
// ============================================================

static int js_abstract_eq(JSValue a, JSValue b) {
    if (js_is_number(a) && js_is_number(b)) return js_as_number(a) == js_as_number(b);
    if (js_is_string(a) && js_is_string(b)) return js_string_equals(js_as_string(a), js_as_string(b));
    if (js_is_bool(a) && js_is_bool(b)) return js_as_bool(a) == js_as_bool(b);
    if (js_is_null(a) && js_is_null(b)) return 1;
    if (js_is_undefined(a) && js_is_undefined(b)) return 1;
    if (js_is_null(a) && js_is_undefined(b)) return 1;
    if (js_is_undefined(a) && js_is_null(b)) return 1;
    if (js_is_number(a) && js_is_string(b)) return js_as_number(a) == js_to_number(b);
    if (js_is_string(a) && js_is_number(b)) return js_to_number(a) == js_as_number(b);
    if (js_is_bool(a)) return js_abstract_eq(js_number(js_as_bool(a) ? 1.0 : 0.0), b);
    if (js_is_bool(b)) return js_abstract_eq(a, js_number(js_as_bool(b) ? 1.0 : 0.0));
    return 0;
}

static int js_strict_eq(JSValue a, JSValue b) {
    if (js_is_number(a) && js_is_number(b)) return js_as_number(a) == js_as_number(b);
    if (js_is_string(a) && js_is_string(b)) return js_string_equals(js_as_string(a), js_as_string(b));
    return (uint64_t)a == (uint64_t)b;
}

JSValue js_eq(JSValue a, JSValue b)   { return js_abstract_eq(a, b) ? JS_TRUE : JS_FALSE; }
JSValue js_neq(JSValue a, JSValue b)  { return js_abstract_eq(a, b) ? JS_FALSE : JS_TRUE; }
JSValue js_seq(JSValue a, JSValue b)  { return js_strict_eq(a, b) ? JS_TRUE : JS_FALSE; }
JSValue js_sneq(JSValue a, JSValue b) { return js_strict_eq(a, b) ? JS_FALSE : JS_TRUE; }

JSValue js_lt(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) < 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) < js_to_number(b) ? JS_TRUE : JS_FALSE;
}
JSValue js_gt(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) > 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) > js_to_number(b) ? JS_TRUE : JS_FALSE;
}
JSValue js_lte(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) <= 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) <= js_to_number(b) ? JS_TRUE : JS_FALSE;
}
JSValue js_gte(JSValue a, JSValue b) {
    if (js_is_string(a) && js_is_string(b))
        return js_string_compare(js_as_string(a), js_as_string(b)) >= 0 ? JS_TRUE : JS_FALSE;
    return js_to_number(a) >= js_to_number(b) ? JS_TRUE : JS_FALSE;
}

// ============================================================
// Console I/O
// ============================================================

void js_console_log(JSValue* args, int argc) {
    for (int i = 0; i < argc; i++) {
        if (i > 0) printf(" ");
        char* s = js_to_cstring(args[i]);
        printf("%s", s);
        free(s);
    }
    printf("\n");
    fflush(stdout);
}

void js_console_error(JSValue* args, int argc) {
    for (int i = 0; i < argc; i++) {
        if (i > 0) fprintf(stderr, " ");
        char* s = js_to_cstring(args[i]);
        fprintf(stderr, "%s", s);
        free(s);
    }
    fprintf(stderr, "\n");
    fflush(stderr);
}

// ============================================================
// Property access (generic)
// ============================================================

JSValue js_get_prop(JSValue obj, JSValue key) {
    if (js_is_object(obj)) {
        char* ks = js_to_cstring(key);
        JSValue r = js_object_get((JSObject*)js_as_ptr(obj), ks);
        free(ks);
        return r;
    }
    if (js_is_array(obj)) {
        if (js_is_number(key)) {
            double n = js_as_number(key);
            if (n >= 0 && n == floor(n))
                return js_array_get_index(obj, (int)n);
        }
        char* ks = js_to_cstring(key);
        if (strcmp(ks, "length") == 0) {
            JSArray* arr = (JSArray*)js_as_ptr(obj);
            free(ks);
            return js_number((double)arr->length);
        }
        free(ks);
        return JS_UNDEFINED;
    }
    if (js_is_string(obj)) {
        char* ks = js_to_cstring(key);
        if (strcmp(ks, "length") == 0) {
            free(ks);
            return js_number((double)js_as_string(obj)->length);
        }
        if (js_is_number(key)) {
            int idx = (int)js_as_number(key);
            JSString* s = js_as_string(obj);
            if (idx >= 0 && idx < s->length) {
                free(ks);
                return js_string_from_len(&s->data[idx], 1);
            }
        }
        free(ks);
        return JS_UNDEFINED;
    }
    return JS_UNDEFINED;
}

void js_set_prop(JSValue obj, JSValue key, JSValue value) {
    if (js_is_object(obj)) {
        char* ks = js_to_cstring(key);
        js_object_set((JSObject*)js_as_ptr(obj), ks, value);
        free(ks);
    } else if (js_is_array(obj)) {
        if (js_is_number(key)) {
            int idx = (int)js_as_number(key);
            js_array_set_index(obj, idx, value);
        }
    }
}

// ============================================================
// Method dispatch
// ============================================================

JSValue js_call_method(JSValue this_val, const char* method, JSValue* args, int argc) {
    // --- String methods ---
    if (js_is_string(this_val)) {
        JSString* s = js_as_string(this_val);
        if (strcmp(method, "charAt") == 0) {
            int idx = argc > 0 ? (int)js_to_number(args[0]) : 0;
            if (idx >= 0 && idx < s->length) return js_string_from_len(&s->data[idx], 1);
            return js_string_from_cstr("");
        }
        if (strcmp(method, "charCodeAt") == 0) {
            int idx = argc > 0 ? (int)js_to_number(args[0]) : 0;
            if (idx >= 0 && idx < s->length) return js_number((double)(unsigned char)s->data[idx]);
            return js_number(NAN);
        }
        if (strcmp(method, "indexOf") == 0) {
            if (argc == 0) return js_number(-1);
            char* search = js_to_cstring(args[0]);
            char* found = strstr(s->data, search);
            int r = found ? (int)(found - s->data) : -1;
            free(search);
            return js_number((double)r);
        }
        if (strcmp(method, "includes") == 0) {
            if (argc == 0) return JS_FALSE;
            char* search = js_to_cstring(args[0]);
            int found = strstr(s->data, search) != NULL;
            free(search);
            return found ? JS_TRUE : JS_FALSE;
        }
        if (strcmp(method, "slice") == 0) {
            int start = argc > 0 ? (int)js_to_number(args[0]) : 0;
            int end = argc > 1 ? (int)js_to_number(args[1]) : s->length;
            if (start < 0) start += s->length;
            if (end < 0) end += s->length;
            if (start < 0) start = 0;
            if (end > s->length) end = s->length;
            if (start >= end) return js_string_from_cstr("");
            return js_string_from_len(&s->data[start], end - start);
        }
        if (strcmp(method, "substring") == 0) {
            int start = argc > 0 ? (int)js_to_number(args[0]) : 0;
            int end = argc > 1 ? (int)js_to_number(args[1]) : s->length;
            if (start < 0) start = 0; if (end < 0) end = 0;
            if (start > s->length) start = s->length;
            if (end > s->length) end = s->length;
            if (start > end) { int t = start; start = end; end = t; }
            return js_string_from_len(&s->data[start], end - start);
        }
        if (strcmp(method, "toUpperCase") == 0) {
            JSString* r = js_string_alloc(s->data, s->length);
            for (int i = 0; i < r->length; i++)
                if (r->data[i] >= 'a' && r->data[i] <= 'z') r->data[i] -= 32;
            return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
        }
        if (strcmp(method, "toLowerCase") == 0) {
            JSString* r = js_string_alloc(s->data, s->length);
            for (int i = 0; i < r->length; i++)
                if (r->data[i] >= 'A' && r->data[i] <= 'Z') r->data[i] += 32;
            return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
        }
        if (strcmp(method, "trim") == 0) {
            int start = 0, end = s->length;
            while (start < end && (s->data[start] == ' ' || s->data[start] == '\t' || s->data[start] == '\n' || s->data[start] == '\r')) start++;
            while (end > start && (s->data[end-1] == ' ' || s->data[end-1] == '\t' || s->data[end-1] == '\n' || s->data[end-1] == '\r')) end--;
            return js_string_from_len(&s->data[start], end - start);
        }
        if (strcmp(method, "split") == 0) {
            JSValue arr = js_array_new();
            if (argc == 0) { js_array_push_val(arr, this_val); return arr; }
            char* sep = js_to_cstring(args[0]);
            int slen = (int)strlen(sep);
            if (slen == 0) {
                for (int i = 0; i < s->length; i++)
                    js_array_push_val(arr, js_string_from_len(&s->data[i], 1));
            } else {
                const char* p = s->data;
                const char* end_ptr = s->data + s->length;
                while (p <= end_ptr) {
                    const char* f = strstr(p, sep);
                    if (!f) f = end_ptr;
                    js_array_push_val(arr, js_string_from_len(p, (int)(f - p)));
                    p = f + slen;
                    if (f == end_ptr) break;
                }
            }
            free(sep);
            return arr;
        }
        if (strcmp(method, "startsWith") == 0) {
            if (argc == 0) return JS_FALSE;
            char* search = js_to_cstring(args[0]);
            int slen = (int)strlen(search);
            int r = (slen <= s->length && memcmp(s->data, search, slen) == 0);
            free(search);
            return r ? JS_TRUE : JS_FALSE;
        }
        if (strcmp(method, "endsWith") == 0) {
            if (argc == 0) return JS_FALSE;
            char* search = js_to_cstring(args[0]);
            int slen = (int)strlen(search);
            int r = (slen <= s->length && memcmp(s->data + s->length - slen, search, slen) == 0);
            free(search);
            return r ? JS_TRUE : JS_FALSE;
        }
        if (strcmp(method, "repeat") == 0) {
            int count = argc > 0 ? (int)js_to_number(args[0]) : 0;
            if (count <= 0) return js_string_from_cstr("");
            int nl = s->length * count;
            JSString* r = js_string_alloc(NULL, nl);
            for (int i = 0; i < count; i++) memcpy(r->data + i * s->length, s->data, s->length);
            r->data[nl] = '\0';
            return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
        }
        if (strcmp(method, "replace") == 0) {
            if (argc < 2) return this_val;
            char* search = js_to_cstring(args[0]);
            char* repl = js_to_cstring(args[1]);
            char* f = strstr(s->data, search);
            if (!f) { free(search); free(repl); return this_val; }
            int slen = (int)strlen(search), rlen = (int)strlen(repl);
            int nl = s->length - slen + rlen;
            JSString* r = js_string_alloc(NULL, nl);
            int pre = (int)(f - s->data);
            memcpy(r->data, s->data, pre);
            memcpy(r->data + pre, repl, rlen);
            memcpy(r->data + pre + rlen, f + slen, s->length - pre - slen);
            r->data[nl] = '\0';
            free(search); free(repl);
            return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
        }
        if (strcmp(method, "padStart") == 0) {
            int target = argc > 0 ? (int)js_to_number(args[0]) : 0;
            if (target <= s->length) return this_val;
            char* pad = argc > 1 ? js_to_cstring(args[1]) : _strdup(" ");
            int plen = (int)strlen(pad);
            int nl = target;
            JSString* r = js_string_alloc(NULL, nl);
            int fill = target - s->length;
            for (int i = 0; i < fill; i++) r->data[i] = pad[i % plen];
            memcpy(r->data + fill, s->data, s->length);
            r->data[nl] = '\0';
            free(pad);
            return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
        }
        if (strcmp(method, "padEnd") == 0) {
            int target = argc > 0 ? (int)js_to_number(args[0]) : 0;
            if (target <= s->length) return this_val;
            char* pad = argc > 1 ? js_to_cstring(args[1]) : _strdup(" ");
            int plen = (int)strlen(pad);
            int nl = target;
            JSString* r = js_string_alloc(NULL, nl);
            memcpy(r->data, s->data, s->length);
            int fill = target - s->length;
            for (int i = 0; i < fill; i++) r->data[s->length + i] = pad[i % plen];
            r->data[nl] = '\0';
            free(pad);
            return (JSValue)(STRING_TAG | ((uint64_t)r & PTR_MASK));
        }
    }

    // --- Array methods ---
    if (js_is_array(this_val)) {
        JSArray* arr = (JSArray*)js_as_ptr(this_val);
        if (strcmp(method, "push") == 0) {
            for (int i = 0; i < argc; i++) js_array_push_val(this_val, args[i]);
            return js_number((double)arr->length);
        }
        if (strcmp(method, "pop") == 0) { return js_array_pop_val(this_val); }
        if (strcmp(method, "shift") == 0) {
            if (arr->length == 0) return JS_UNDEFINED;
            JSValue first = arr->data[0];
            memmove(arr->data, arr->data + 1, (arr->length - 1) * sizeof(JSValue));
            arr->length--;
            return first;
        }
        if (strcmp(method, "unshift") == 0) {
            while (arr->length + argc > arr->capacity) js_array_grow(arr);
            memmove(arr->data + argc, arr->data, arr->length * sizeof(JSValue));
            for (int i = 0; i < argc; i++) arr->data[i] = args[i];
            arr->length += argc;
            return js_number((double)arr->length);
        }
        if (strcmp(method, "indexOf") == 0) {
            if (argc == 0) return js_number(-1);
            for (int i = 0; i < arr->length; i++)
                if (js_strict_eq(arr->data[i], args[0])) return js_number((double)i);
            return js_number(-1);
        }
        if (strcmp(method, "includes") == 0) {
            if (argc == 0) return JS_FALSE;
            for (int i = 0; i < arr->length; i++)
                if (js_strict_eq(arr->data[i], args[0])) return JS_TRUE;
            return JS_FALSE;
        }
        if (strcmp(method, "join") == 0) {
            char* sep = argc > 0 ? js_to_cstring(args[0]) : _strdup(",");
            int slen = (int)strlen(sep);
            char** parts = (char**)malloc(arr->length * sizeof(char*));
            int total = 0;
            for (int i = 0; i < arr->length; i++) {
                parts[i] = js_to_cstring(arr->data[i]);
                total += (int)strlen(parts[i]);
            }
            if (arr->length > 1) total += (arr->length - 1) * slen;
            char* r = (char*)malloc(total + 1); r[0] = '\0';
            for (int i = 0; i < arr->length; i++) {
                if (i > 0) strcat(r, sep);
                strcat(r, parts[i]);
                free(parts[i]);
            }
            free(parts); free(sep);
            JSValue rv = js_string_from_cstr(r); free(r);
            return rv;
        }
        if (strcmp(method, "reverse") == 0) {
            for (int i = 0, j = arr->length - 1; i < j; i++, j--) {
                JSValue t = arr->data[i]; arr->data[i] = arr->data[j]; arr->data[j] = t;
            }
            return this_val;
        }
        if (strcmp(method, "slice") == 0) {
            int start = argc > 0 ? (int)js_to_number(args[0]) : 0;
            int end = argc > 1 ? (int)js_to_number(args[1]) : arr->length;
            if (start < 0) start += arr->length;
            if (end < 0) end += arr->length;
            if (start < 0) start = 0;
            if (end > arr->length) end = arr->length;
            JSValue r = js_array_new();
            for (int i = start; i < end; i++) js_array_push_val(r, arr->data[i]);
            return r;
        }
        if (strcmp(method, "concat") == 0) {
            JSValue r = js_array_new();
            for (int i = 0; i < arr->length; i++) js_array_push_val(r, arr->data[i]);
            for (int a = 0; a < argc; a++) {
                if (js_is_array(args[a])) {
                    JSArray* o = (JSArray*)js_as_ptr(args[a]);
                    for (int i = 0; i < o->length; i++) js_array_push_val(r, o->data[i]);
                } else {
                    js_array_push_val(r, args[a]);
                }
            }
            return r;
        }
        // Higher-order array methods (require function values)
        if (strcmp(method, "forEach") == 0) {
            if (argc == 0 || !js_is_function(args[0])) return JS_UNDEFINED;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                fn->fn(ca, 3, fn->closure_env);
            }
            return JS_UNDEFINED;
        }
        if (strcmp(method, "map") == 0) {
            JSValue r = js_array_new();
            if (argc == 0 || !js_is_function(args[0])) return r;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                js_array_push_val(r, fn->fn(ca, 3, fn->closure_env));
            }
            return r;
        }
        if (strcmp(method, "filter") == 0) {
            JSValue r = js_array_new();
            if (argc == 0 || !js_is_function(args[0])) return r;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                if (js_is_truthy(fn->fn(ca, 3, fn->closure_env)))
                    js_array_push_val(r, arr->data[i]);
            }
            return r;
        }
        if (strcmp(method, "reduce") == 0) {
            if (argc == 0 || !js_is_function(args[0])) return JS_UNDEFINED;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            int si = 0;
            JSValue acc;
            if (argc > 1) { acc = args[1]; } else {
                if (arr->length == 0) { fprintf(stderr, "TypeError: Reduce of empty array with no initial value\n"); exit(1); }
                acc = arr->data[0]; si = 1;
            }
            for (int i = si; i < arr->length; i++) {
                JSValue ca[4] = { acc, arr->data[i], js_number((double)i), this_val };
                acc = fn->fn(ca, 4, fn->closure_env);
            }
            return acc;
        }
        if (strcmp(method, "find") == 0) {
            if (argc == 0 || !js_is_function(args[0])) return JS_UNDEFINED;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                if (js_is_truthy(fn->fn(ca, 3, fn->closure_env))) return arr->data[i];
            }
            return JS_UNDEFINED;
        }
        if (strcmp(method, "findIndex") == 0) {
            if (argc == 0 || !js_is_function(args[0])) return js_number(-1);
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                if (js_is_truthy(fn->fn(ca, 3, fn->closure_env))) return js_number((double)i);
            }
            return js_number(-1);
        }
        if (strcmp(method, "every") == 0) {
            if (argc == 0 || !js_is_function(args[0])) return JS_TRUE;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                if (!js_is_truthy(fn->fn(ca, 3, fn->closure_env))) return JS_FALSE;
            }
            return JS_TRUE;
        }
        if (strcmp(method, "some") == 0) {
            if (argc == 0 || !js_is_function(args[0])) return JS_FALSE;
            JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
            for (int i = 0; i < arr->length; i++) {
                JSValue ca[3] = { arr->data[i], js_number((double)i), this_val };
                if (js_is_truthy(fn->fn(ca, 3, fn->closure_env))) return JS_TRUE;
            }
            return JS_FALSE;
        }
        if (strcmp(method, "flat") == 0) {
            JSValue r = js_array_new();
            for (int i = 0; i < arr->length; i++) {
                if (js_is_array(arr->data[i])) {
                    JSArray* inner = (JSArray*)js_as_ptr(arr->data[i]);
                    for (int j = 0; j < inner->length; j++) js_array_push_val(r, inner->data[j]);
                } else {
                    js_array_push_val(r, arr->data[i]);
                }
            }
            return r;
        }
    }

    // --- Object methods ---
    if (js_is_object(this_val)) {
        JSObject* obj = (JSObject*)js_as_ptr(this_val);
        if (strcmp(method, "hasOwnProperty") == 0) {
            if (argc == 0) return JS_FALSE;
            char* ks = js_to_cstring(args[0]);
            unsigned int idx = hash_string(ks) & (obj->capacity - 1);
            unsigned int start = idx;
            int found = 0;
            while (obj->entries[idx].occupied) {
                if (obj->entries[idx].occupied == 1 && strcmp(obj->entries[idx].key, ks) == 0) { found = 1; break; }
                idx = (idx + 1) & (obj->capacity - 1);
                if (idx == start) break;
            }
            free(ks);
            return found ? JS_TRUE : JS_FALSE;
        }
    }

    // User-defined method: look up function-valued property and call with this
    if (js_is_object(this_val)) {
        JSValue method_val = js_object_get((JSObject*)js_as_ptr(this_val), method);
        if (js_is_function(method_val)) {
            JSFunction* fn = (JSFunction*)js_as_ptr(method_val);
            js_this_push(this_val);
            JSValue result = fn->fn(args, argc, fn->closure_env);
            js_this_pop();
            return result;
        }
    }

    fprintf(stderr, "TypeError: %s is not a function\n", method);
    exit(1);
    return JS_UNDEFINED;
}

// ============================================================
// Function/closure support
// ============================================================

JSValue js_func_new(JSNativeFunc fn, void* closure_env, int arity) {
    JSFunction* f = (JSFunction*)malloc(sizeof(JSFunction));
    f->header.type = HEAP_FUNCTION;
    f->header.refcount = 1;
    f->fn = fn;
    f->closure_env = closure_env;
    f->arity = arity;
    return (JSValue)(FUNC_TAG | ((uint64_t)f & PTR_MASK));
}

JSValue js_call_func(JSValue func_val, JSValue* args, int argc) {
    if (!js_is_function(func_val)) {
        fprintf(stderr, "TypeError: not a function\n");
        exit(1);
    }
    JSFunction* fn = (JSFunction*)js_as_ptr(func_val);
    return fn->fn(args, argc, fn->closure_env);
}

// ============================================================
// Built-in functions
// ============================================================

JSValue js_prompt(JSValue message) {
    if (!js_is_undefined(message)) {
        char* s = js_to_cstring(message);
        printf("%s", s);
        free(s);
        fflush(stdout);
    }
    char buf[4096];
    if (fgets(buf, sizeof(buf), stdin)) {
        int len = (int)strlen(buf);
        if (len > 0 && buf[len-1] == '\n') buf[--len] = '\0';
        if (len > 0 && buf[len-1] == '\r') buf[--len] = '\0';
        return js_string_from_cstr(buf);
    }
    return JS_NULL;
}

JSValue js_parse_int(JSValue str, JSValue radix) {
    char* s = js_to_cstring(str);
    int base = js_is_undefined(radix) ? 10 : (int)js_to_number(radix);
    if (base == 0) base = 10;
    char* end;
    // Skip leading whitespace
    char* p = s;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    long r = strtol(p, &end, base);
    int ok = (end != p);
    free(s);
    return ok ? js_number((double)r) : js_number(NAN);
}

JSValue js_parse_float(JSValue str) {
    char* s = js_to_cstring(str);
    char* end;
    double r = strtod(s, &end);
    int ok = (end != s);
    free(s);
    return ok ? js_number(r) : js_number(NAN);
}

JSValue js_isnan(JSValue v) { return isnan(js_to_number(v)) ? JS_TRUE : JS_FALSE; }
JSValue js_isfinite(JSValue v) {
    double n = js_to_number(v);
    return (!isnan(n) && !isinf(n)) ? JS_TRUE : JS_FALSE;
}

// Math
JSValue js_math_floor(JSValue v) { return js_number(floor(js_to_number(v))); }
JSValue js_math_ceil(JSValue v)  { return js_number(ceil(js_to_number(v))); }
JSValue js_math_round(JSValue v) { return js_number(round(js_to_number(v))); }
JSValue js_math_sqrt(JSValue v)  { return js_number(sqrt(js_to_number(v))); }
JSValue js_math_abs(JSValue v)   { return js_number(fabs(js_to_number(v))); }
JSValue js_math_pow(JSValue b, JSValue e) { return js_number(pow(js_to_number(b), js_to_number(e))); }
JSValue js_math_log(JSValue v)   { return js_number(log(js_to_number(v))); }
JSValue js_math_log2(JSValue v)  { return js_number(log2(js_to_number(v))); }
JSValue js_math_log10(JSValue v) { return js_number(log10(js_to_number(v))); }
JSValue js_math_sin(JSValue v)   { return js_number(sin(js_to_number(v))); }
JSValue js_math_cos(JSValue v)   { return js_number(cos(js_to_number(v))); }
JSValue js_math_tan(JSValue v)   { return js_number(tan(js_to_number(v))); }
JSValue js_math_atan2(JSValue y, JSValue x) { return js_number(atan2(js_to_number(y), js_to_number(x))); }
JSValue js_math_exp(JSValue v)   { return js_number(exp(js_to_number(v))); }
JSValue js_math_trunc(JSValue v) { return js_number(trunc(js_to_number(v))); }
JSValue js_math_sign(JSValue v) {
    double n = js_to_number(v);
    if (isnan(n)) return js_number(NAN);
    if (n > 0) return js_number(1);
    if (n < 0) return js_number(-1);
    return js_number(n);
}

static int js_rand_seeded = 0;
JSValue js_math_random(void) {
    if (!js_rand_seeded) { srand((unsigned int)time(NULL)); js_rand_seeded = 1; }
    return js_number((double)rand() / (double)RAND_MAX);
}

JSValue js_math_min(JSValue* args, int argc) {
    if (argc == 0) return js_number(INFINITY);
    double r = js_to_number(args[0]);
    for (int i = 1; i < argc; i++) {
        double n = js_to_number(args[i]);
        if (isnan(n)) return js_number(NAN);
        if (n < r) r = n;
    }
    return js_number(r);
}

JSValue js_math_max(JSValue* args, int argc) {
    if (argc == 0) return js_number(-INFINITY);
    double r = js_to_number(args[0]);
    for (int i = 1; i < argc; i++) {
        double n = js_to_number(args[i]);
        if (isnan(n)) return js_number(NAN);
        if (n > r) r = n;
    }
    return js_number(r);
}

JSValue js_Number(JSValue v) { return js_to_number_val(v); }
JSValue js_String(JSValue v) { return js_to_string_val(v); }
JSValue js_Boolean(JSValue v) { return js_is_truthy(v) ? JS_TRUE : JS_FALSE; }

// Date.now()
JSValue js_date_now(void) {
#ifdef _WIN32
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    uint64_t t = ((uint64_t)ft.dwHighDateTime << 32) | ft.dwLowDateTime;
    t -= 116444736000000000ULL;
    t /= 10000;
    return js_number((double)t);
#else
    struct timeval tv;
    gettimeofday(&tv, NULL);
    return js_number((double)tv.tv_sec * 1000.0 + (double)tv.tv_usec / 1000.0);
#endif
}

// JSON.stringify (basic)
static void json_append(char** buf, int* len, int* cap, const char* s, int slen) {
    while (*len + slen >= *cap) { *cap *= 2; *buf = (char*)realloc(*buf, *cap); }
    memcpy(*buf + *len, s, slen);
    *len += slen;
}

static void json_impl(JSValue v, char** buf, int* len, int* cap) {
    if (js_is_null(v) || js_is_undefined(v)) {
        json_append(buf, len, cap, "null", 4);
    } else if (js_is_bool(v)) {
        const char* s = js_as_bool(v) ? "true" : "false";
        json_append(buf, len, cap, s, (int)strlen(s));
    } else if (js_is_number(v)) {
        double n = js_as_number(v);
        if (isnan(n) || isinf(n)) { json_append(buf, len, cap, "null", 4); return; }
        char tmp[64];
        int tlen = snprintf(tmp, sizeof(tmp), "%.17g", n);
        json_append(buf, len, cap, tmp, tlen);
    } else if (js_is_string(v)) {
        JSString* s = js_as_string(v);
        json_append(buf, len, cap, "\"", 1);
        for (int i = 0; i < s->length; i++) {
            char c = s->data[i];
            switch (c) {
                case '"':  json_append(buf, len, cap, "\\\"", 2); break;
                case '\\': json_append(buf, len, cap, "\\\\", 2); break;
                case '\n': json_append(buf, len, cap, "\\n", 2); break;
                case '\r': json_append(buf, len, cap, "\\r", 2); break;
                case '\t': json_append(buf, len, cap, "\\t", 2); break;
                default:   json_append(buf, len, cap, &c, 1); break;
            }
        }
        json_append(buf, len, cap, "\"", 1);
    } else if (js_is_array(v)) {
        JSArray* arr = (JSArray*)js_as_ptr(v);
        json_append(buf, len, cap, "[", 1);
        for (int i = 0; i < arr->length; i++) {
            if (i > 0) json_append(buf, len, cap, ",", 1);
            json_impl(arr->data[i], buf, len, cap);
        }
        json_append(buf, len, cap, "]", 1);
    } else if (js_is_object(v)) {
        JSObject* obj = (JSObject*)js_as_ptr(v);
        json_append(buf, len, cap, "{", 1);
        int first = 1;
        for (int i = 0; i < obj->capacity; i++) {
            if (obj->entries[i].occupied == 1) {
                if (!first) json_append(buf, len, cap, ",", 1);
                first = 0;
                json_append(buf, len, cap, "\"", 1);
                json_append(buf, len, cap, obj->entries[i].key, (int)strlen(obj->entries[i].key));
                json_append(buf, len, cap, "\":", 2);
                json_impl(obj->entries[i].value, buf, len, cap);
            }
        }
        json_append(buf, len, cap, "}", 1);
    } else {
        json_append(buf, len, cap, "null", 4);
    }
}

JSValue js_json_stringify(JSValue v) {
    int cap = 256, len = 0;
    char* buf = (char*)malloc(cap);
    json_impl(v, &buf, &len, &cap);
    buf[len] = '\0';
    JSValue r = js_string_from_cstr(buf);
    free(buf);
    return r;
}

// Object.keys
JSValue js_object_keys(JSValue v) {
    JSValue arr = js_array_new();
    if (!js_is_object(v)) return arr;
    JSObject* obj = (JSObject*)js_as_ptr(v);
    for (int i = 0; i < obj->capacity; i++) {
        if (obj->entries[i].occupied == 1)
            js_array_push_val(arr, js_string_from_cstr(obj->entries[i].key));
    }
    return arr;
}

// Object.values
JSValue js_object_values(JSValue v) {
    JSValue arr = js_array_new();
    if (!js_is_object(v)) return arr;
    JSObject* obj = (JSObject*)js_as_ptr(v);
    for (int i = 0; i < obj->capacity; i++) {
        if (obj->entries[i].occupied == 1)
            js_array_push_val(arr, obj->entries[i].value);
    }
    return arr;
}

// Array.isArray
JSValue js_array_is_array(JSValue v) {
    return js_is_array(v) ? JS_TRUE : JS_FALSE;
}

// ============================================================
// JSON.parse
// ============================================================

typedef struct { const char* src; int pos; int len; } JSONParser;

static void json_skip_ws(JSONParser* p) {
    while (p->pos < p->len) {
        char c = p->src[p->pos];
        if (c == ' ' || c == '\t' || c == '\n' || c == '\r') p->pos++;
        else break;
    }
}

static JSValue json_parse_value(JSONParser* p);

static JSValue json_parse_string(JSONParser* p) {
    p->pos++; // skip opening "
    int cap = 64, len = 0;
    char* buf = (char*)malloc(cap);
    while (p->pos < p->len && p->src[p->pos] != '"') {
        char c = p->src[p->pos++];
        if (c == '\\' && p->pos < p->len) {
            c = p->src[p->pos++];
            switch (c) {
                case '"': c = '"'; break;
                case '\\': c = '\\'; break;
                case '/': c = '/'; break;
                case 'n': c = '\n'; break;
                case 'r': c = '\r'; break;
                case 't': c = '\t'; break;
                case 'b': c = '\b'; break;
                case 'f': c = '\f'; break;
                case 'u': {
                    // Basic \uXXXX — decode to UTF-8
                    unsigned int cp = 0;
                    for (int i = 0; i < 4 && p->pos < p->len; i++, p->pos++) {
                        char h = p->src[p->pos];
                        cp <<= 4;
                        if (h >= '0' && h <= '9') cp |= h - '0';
                        else if (h >= 'a' && h <= 'f') cp |= h - 'a' + 10;
                        else if (h >= 'A' && h <= 'F') cp |= h - 'A' + 10;
                    }
                    if (cp < 0x80) {
                        if (len + 1 >= cap) { cap *= 2; buf = (char*)realloc(buf, cap); }
                        buf[len++] = (char)cp;
                    } else if (cp < 0x800) {
                        if (len + 2 >= cap) { cap *= 2; buf = (char*)realloc(buf, cap); }
                        buf[len++] = (char)(0xC0 | (cp >> 6));
                        buf[len++] = (char)(0x80 | (cp & 0x3F));
                    } else {
                        if (len + 3 >= cap) { cap *= 2; buf = (char*)realloc(buf, cap); }
                        buf[len++] = (char)(0xE0 | (cp >> 12));
                        buf[len++] = (char)(0x80 | ((cp >> 6) & 0x3F));
                        buf[len++] = (char)(0x80 | (cp & 0x3F));
                    }
                    continue;
                }
                default: break;
            }
        }
        if (len + 1 >= cap) { cap *= 2; buf = (char*)realloc(buf, cap); }
        buf[len++] = c;
    }
    if (p->pos < p->len) p->pos++; // skip closing "
    buf[len] = '\0';
    JSValue r = js_string_from_len(buf, len);
    free(buf);
    return r;
}

static JSValue json_parse_number(JSONParser* p) {
    int start = p->pos;
    if (p->src[p->pos] == '-') p->pos++;
    while (p->pos < p->len && p->src[p->pos] >= '0' && p->src[p->pos] <= '9') p->pos++;
    if (p->pos < p->len && p->src[p->pos] == '.') {
        p->pos++;
        while (p->pos < p->len && p->src[p->pos] >= '0' && p->src[p->pos] <= '9') p->pos++;
    }
    if (p->pos < p->len && (p->src[p->pos] == 'e' || p->src[p->pos] == 'E')) {
        p->pos++;
        if (p->pos < p->len && (p->src[p->pos] == '+' || p->src[p->pos] == '-')) p->pos++;
        while (p->pos < p->len && p->src[p->pos] >= '0' && p->src[p->pos] <= '9') p->pos++;
    }
    char tmp[64];
    int slen = p->pos - start;
    if (slen >= 64) slen = 63;
    memcpy(tmp, p->src + start, slen);
    tmp[slen] = '\0';
    return js_number(strtod(tmp, NULL));
}

static JSValue json_parse_array(JSONParser* p) {
    p->pos++; // skip [
    JSValue arr = js_array_new();
    json_skip_ws(p);
    if (p->pos < p->len && p->src[p->pos] == ']') { p->pos++; return arr; }
    while (p->pos < p->len) {
        json_skip_ws(p);
        js_array_push_val(arr, json_parse_value(p));
        json_skip_ws(p);
        if (p->pos < p->len && p->src[p->pos] == ',') { p->pos++; continue; }
        break;
    }
    if (p->pos < p->len && p->src[p->pos] == ']') p->pos++;
    return arr;
}

static JSValue json_parse_object(JSONParser* p) {
    p->pos++; // skip {
    JSValue obj = js_object_new();
    json_skip_ws(p);
    if (p->pos < p->len && p->src[p->pos] == '}') { p->pos++; return obj; }
    while (p->pos < p->len) {
        json_skip_ws(p);
        if (p->pos >= p->len || p->src[p->pos] != '"') break;
        JSValue key = json_parse_string(p);
        json_skip_ws(p);
        if (p->pos < p->len && p->src[p->pos] == ':') p->pos++;
        json_skip_ws(p);
        JSValue val = json_parse_value(p);
        char* ks = js_to_cstring(key);
        js_object_set((JSObject*)js_as_ptr(obj), ks, val);
        free(ks);
        json_skip_ws(p);
        if (p->pos < p->len && p->src[p->pos] == ',') { p->pos++; continue; }
        break;
    }
    if (p->pos < p->len && p->src[p->pos] == '}') p->pos++;
    return obj;
}

static JSValue json_parse_value(JSONParser* p) {
    json_skip_ws(p);
    if (p->pos >= p->len) return JS_UNDEFINED;
    char c = p->src[p->pos];
    if (c == '"') return json_parse_string(p);
    if (c == '[') return json_parse_array(p);
    if (c == '{') return json_parse_object(p);
    if (c == 't' && p->pos + 4 <= p->len && memcmp(p->src + p->pos, "true", 4) == 0) { p->pos += 4; return JS_TRUE; }
    if (c == 'f' && p->pos + 5 <= p->len && memcmp(p->src + p->pos, "false", 5) == 0) { p->pos += 5; return JS_FALSE; }
    if (c == 'n' && p->pos + 4 <= p->len && memcmp(p->src + p->pos, "null", 4) == 0) { p->pos += 4; return JS_NULL; }
    if (c == '-' || (c >= '0' && c <= '9')) return json_parse_number(p);
    // Invalid JSON
    js_throw(js_string_from_cstr("SyntaxError: Unexpected token in JSON"));
    return JS_UNDEFINED;
}

JSValue js_json_parse(JSValue str) {
    char* s = js_to_cstring(str);
    JSONParser p = { s, 0, (int)strlen(s) };
    JSValue result = json_parse_value(&p);
    free(s);
    return result;
}

// ============================================================
// Spread helpers
// ============================================================

void js_array_concat_into(JSValue target, JSValue source) {
    if (!js_is_array(source)) {
        js_array_push_val(target, source);
        return;
    }
    JSArray* src = (JSArray*)js_as_ptr(source);
    for (int i = 0; i < src->length; i++) {
        js_array_push_val(target, src->data[i]);
    }
}

void js_object_spread(JSValue target, JSValue source) {
    if (!js_is_object(source)) return;
    JSObject* src = (JSObject*)js_as_ptr(source);
    JSObject* tgt = (JSObject*)js_as_ptr(target);
    for (int i = 0; i < src->capacity; i++) {
        if (src->entries[i].occupied == 1)
            js_object_set(tgt, src->entries[i].key, src->entries[i].value);
    }
}

// ============================================================
// this binding
// ============================================================

#define MAX_THIS_DEPTH 64
static JSValue js_this_stack[MAX_THIS_DEPTH];
static int js_this_sp = 0;

void js_this_push(JSValue v) {
    if (js_this_sp < MAX_THIS_DEPTH) js_this_stack[js_this_sp++] = v;
}

void js_this_pop(void) {
    if (js_this_sp > 0) js_this_sp--;
}

JSValue js_this_get(void) {
    return js_this_sp > 0 ? js_this_stack[js_this_sp - 1] : JS_UNDEFINED;
}

// ============================================================
// Runtime init
// ============================================================

// Allocate closure environment (array of JSValue)
void* js_alloc_closure_env(int count) {
    return calloc(count, sizeof(JSValue));
}

void js_runtime_init(void) {
    srand((unsigned int)time(NULL));
    js_rand_seeded = 1;
}
