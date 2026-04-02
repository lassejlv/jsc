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

