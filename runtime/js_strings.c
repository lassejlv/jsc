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

