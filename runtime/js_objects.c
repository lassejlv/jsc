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
