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
        if (strcmp(method, "replace") == 0 || strcmp(method, "replaceAll") == 0) {
            if (argc < 2) return this_val;
            int replace_all = (strcmp(method, "replaceAll") == 0);
            // Check if first arg is a RegExp
            if (js_is_object(args[0])) {
                JSValue rtype = js_object_get((JSObject*)js_as_ptr(args[0]), "__type");
                if (js_is_string(rtype) && strcmp(js_as_string(rtype)->data, "RegExp") == 0) {
                    JSValue rglobal = js_object_get((JSObject*)js_as_ptr(args[0]), "global");
                    if (js_is_truthy(rglobal)) replace_all = 1;
                    JSValue regex_ptr_val = js_object_get((JSObject*)js_as_ptr(args[0]), "__regex");
                    if (!js_is_undefined(regex_ptr_val)) {
                        regex_t* compiled = (regex_t*)((uint64_t)regex_ptr_val & PTR_MASK);
                        char* repl = js_to_cstring(args[1]);
                        int rlen = (int)strlen(repl);
                        // Build result
                        int cap = s->length * 2 + 64;
                        char* result = (char*)malloc(cap);
                        int pos = 0, out = 0;
                        regmatch_t m;
                        while (pos <= s->length && regexec(compiled, s->data + pos, 1, &m, pos > 0 ? REG_NOTBOL : 0) == 0) {
                            int start = m.rm_so, end = m.rm_eo;
                            while (out + (start) + rlen + 1 >= cap) { cap *= 2; result = (char*)realloc(result, cap); }
                            memcpy(result + out, s->data + pos, start);
                            out += start;
                            memcpy(result + out, repl, rlen);
                            out += rlen;
                            pos += end > 0 ? end : 1;
                            if (!replace_all) break;
                        }
                        // Copy remaining
                        int remaining = s->length - pos;
                        while (out + remaining + 1 >= cap) { cap *= 2; result = (char*)realloc(result, cap); }
                        memcpy(result + out, s->data + pos, remaining);
                        out += remaining;
                        result[out] = '\0';
                        free(repl);
                        JSValue r = js_string_from_len(result, out);
                        free(result);
                        return r;
                    }
                }
            }
            // String search/replace
            char* search = js_to_cstring(args[0]);
            char* repl = js_to_cstring(args[1]);
            if (replace_all) {
                int slen = (int)strlen(search), rlen = (int)strlen(repl);
                int cap = s->length * 2 + 64;
                char* result = (char*)malloc(cap);
                int out = 0, pos = 0;
                while (pos <= s->length) {
                    char* f = strstr(s->data + pos, search);
                    if (!f || slen == 0) {
                        int remaining = s->length - pos;
                        while (out + remaining + 1 >= cap) { cap *= 2; result = (char*)realloc(result, cap); }
                        memcpy(result + out, s->data + pos, remaining);
                        out += remaining;
                        break;
                    }
                    int pre = (int)(f - (s->data + pos));
                    while (out + pre + rlen + 1 >= cap) { cap *= 2; result = (char*)realloc(result, cap); }
                    memcpy(result + out, s->data + pos, pre);
                    out += pre;
                    memcpy(result + out, repl, rlen);
                    out += rlen;
                    pos = (int)(f - s->data) + slen;
                }
                result[out] = '\0';
                free(search); free(repl);
                JSValue r = js_string_from_len(result, out);
                free(result);
                return r;
            }
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
        if (strcmp(method, "match") == 0 && argc > 0) {
            // String.match(regexp)
            if (js_is_object(args[0])) {
                JSValue rtype = js_object_get((JSObject*)js_as_ptr(args[0]), "__type");
                if (js_is_string(rtype) && strcmp(js_as_string(rtype)->data, "RegExp") == 0) {
                    return js_regexp_exec(args[0], this_val);
                }
            }
            // Fallback: treat as string search
            return JS_NULL;
        }
        if (strcmp(method, "search") == 0 && argc > 0) {
            if (js_is_object(args[0])) {
                JSValue regex_ptr_val = js_object_get((JSObject*)js_as_ptr(args[0]), "__regex");
                if (!js_is_undefined(regex_ptr_val)) {
                    regex_t* compiled = (regex_t*)((uint64_t)regex_ptr_val & PTR_MASK);
                    regmatch_t m;
                    if (regexec(compiled, s->data, 1, &m, 0) == 0) return js_number((double)m.rm_so);
                    return js_number(-1);
                }
            }
            char* search = js_to_cstring(args[0]);
            char* f = strstr(s->data, search);
            int r = f ? (int)(f - s->data) : -1;
            free(search);
            return js_number((double)r);
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
        if (strcmp(method, "sort") == 0) {
            JSValue cmp = (argc > 0 && js_is_function(args[0])) ? args[0] : JS_UNDEFINED;
            return js_array_sort(this_val, cmp);
        }
        if (strcmp(method, "splice") == 0) {
            return js_array_splice(this_val, args, argc);
        }
        if (strcmp(method, "fill") == 0) {
            JSValue val = argc > 0 ? args[0] : JS_UNDEFINED;
            int start = argc > 1 ? (int)js_to_number(args[1]) : 0;
            int end = argc > 2 ? (int)js_to_number(args[2]) : arr->length;
            if (start < 0) start = arr->length + start;
            if (end < 0) end = arr->length + end;
            for (int i = start; i < end && i < arr->length; i++) arr->data[i] = val;
            return this_val;
        }
        if (strcmp(method, "map") == 0 && argc > 0 && js_is_function(args[0])) {
            // Already handled above, but handle toString on arrays
        }
        if (strcmp(method, "toString") == 0) {
            return js_to_string_val(this_val);
        }
    }

    // --- Object methods ---
    if (js_is_object(this_val)) {
        JSObject* obj = (JSObject*)js_as_ptr(this_val);

        // Map methods
        {
            JSValue mtype = js_object_get(obj, "__type");
            if (js_is_string(mtype) && strcmp(js_as_string(mtype)->data, "Map") == 0) {
                if (strcmp(method, "set") == 0 && argc >= 2) return js_map_set(this_val, args[0], args[1]);
                if (strcmp(method, "get") == 0 && argc >= 1) return js_map_get(this_val, args[0]);
                if (strcmp(method, "has") == 0 && argc >= 1) return js_map_has(this_val, args[0]);
                if (strcmp(method, "delete") == 0 && argc >= 1) return js_map_delete(this_val, args[0]);
                if (strcmp(method, "clear") == 0) {
                    JSMapData* data = map_get_data(this_val);
                    if (data) { data->length = 0; js_object_set(obj, "size", js_number(0)); }
                    return JS_UNDEFINED;
                }
                if (strcmp(method, "forEach") == 0 && argc > 0 && js_is_function(args[0])) {
                    JSMapData* data = map_get_data(this_val);
                    if (data) {
                        JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
                        for (int i = 0; i < data->length; i++) {
                            JSValue cb_args[3] = { data->values[i], data->keys[i], this_val };
                            fn->fn(cb_args, 3, fn->closure_env);
                        }
                    }
                    return JS_UNDEFINED;
                }
                if (strcmp(method, "keys") == 0) {
                    JSMapData* data = map_get_data(this_val);
                    JSValue arr = js_array_new();
                    if (data) for (int i = 0; i < data->length; i++) js_array_push_val(arr, data->keys[i]);
                    return arr;
                }
                if (strcmp(method, "values") == 0) {
                    JSMapData* data = map_get_data(this_val);
                    JSValue arr = js_array_new();
                    if (data) for (int i = 0; i < data->length; i++) js_array_push_val(arr, data->values[i]);
                    return arr;
                }
                if (strcmp(method, "entries") == 0) {
                    JSMapData* data = map_get_data(this_val);
                    JSValue arr = js_array_new();
                    if (data) for (int i = 0; i < data->length; i++) {
                        JSValue pair = js_array_new();
                        js_array_push_val(pair, data->keys[i]);
                        js_array_push_val(pair, data->values[i]);
                        js_array_push_val(arr, pair);
                    }
                    return arr;
                }
            }
            // Set methods
            if (js_is_string(mtype) && strcmp(js_as_string(mtype)->data, "Set") == 0) {
                if (strcmp(method, "add") == 0 && argc >= 1) {
                    JSMapData* data = map_get_data(this_val);
                    if (data && map_find_key(data, args[0]) < 0) {
                        if (data->length >= data->capacity) {
                            data->capacity *= 2;
                            data->keys = (JSValue*)realloc(data->keys, data->capacity * sizeof(JSValue));
                        }
                        data->keys[data->length++] = args[0];
                        js_object_set(obj, "size", js_number((double)data->length));
                    }
                    return this_val;
                }
                if (strcmp(method, "has") == 0 && argc >= 1) {
                    JSMapData* data = map_get_data(this_val);
                    return (data && map_find_key(data, args[0]) >= 0) ? JS_TRUE : JS_FALSE;
                }
                if (strcmp(method, "delete") == 0 && argc >= 1) return js_map_delete(this_val, args[0]);
                if (strcmp(method, "forEach") == 0 && argc > 0 && js_is_function(args[0])) {
                    JSMapData* data = map_get_data(this_val);
                    if (data) {
                        JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
                        for (int i = 0; i < data->length; i++) {
                            JSValue cb_args[2] = { data->keys[i], data->keys[i] };
                            fn->fn(cb_args, 2, fn->closure_env);
                        }
                    }
                    return JS_UNDEFINED;
                }
            }
            // RegExp methods
            if (js_is_string(mtype) && strcmp(js_as_string(mtype)->data, "RegExp") == 0) {
                if (strcmp(method, "test") == 0 && argc >= 1) return js_regexp_test(this_val, args[0]);
                if (strcmp(method, "exec") == 0 && argc >= 1) return js_regexp_exec(this_val, args[0]);
            }
        }

        // Response methods: .text() and .json()
        if (strcmp(method, "text") == 0) {
            JSValue body = js_object_get(obj, "__body");
            if (!js_is_undefined(body)) return body;
        }
        if (strcmp(method, "json") == 0) {
            JSValue body = js_object_get(obj, "__body");
            if (!js_is_undefined(body)) return js_json_parse(body);
        }

        // Promise methods: .then(), .catch(), .finally()
        {
            JSValue __type = js_object_get(obj, "__type");
            if (js_is_string(__type) && strcmp(js_as_string(__type)->data, "Promise") == 0) {
                if (strcmp(method, "then") == 0) {
                    JSValue on_fulfilled = (argc > 0) ? args[0] : JS_UNDEFINED;
                    JSValue on_rejected = (argc > 1) ? args[1] : JS_UNDEFINED;
                    return js_promise_then(this_val, on_fulfilled, on_rejected);
                }
                if (strcmp(method, "catch") == 0) {
                    JSValue on_rejected = (argc > 0) ? args[0] : JS_UNDEFINED;
                    return js_promise_then(this_val, JS_UNDEFINED, on_rejected);
                }
                if (strcmp(method, "finally") == 0) {
                    JSValue on_finally = (argc > 0) ? args[0] : JS_UNDEFINED;
                    return js_promise_then(this_val, on_finally, on_finally);
                }
            }
        }

        // Headers API: .get(key), .has(key), .set(key, val), .delete(key), .forEach(fn)
        // ONLY for Headers objects (have __type "Headers") to avoid conflicts
        {
            JSValue htype = js_object_get(obj, "__type");
            if (js_is_string(htype) && strcmp(js_as_string(htype)->data, "Headers") == 0) {
                if (strcmp(method, "get") == 0 && argc > 0) {
                    char* key = js_to_cstring(args[0]);
                    for (char* p = key; *p; p++) if (*p >= 'A' && *p <= 'Z') *p += 32;
                    JSValue val = js_object_get(obj, key);
                    free(key);
                    return js_is_undefined(val) ? JS_NULL : val;
                }
                if (strcmp(method, "has") == 0 && argc > 0) {
                    char* key = js_to_cstring(args[0]);
                    for (char* p = key; *p; p++) if (*p >= 'A' && *p <= 'Z') *p += 32;
                    JSValue val = js_object_get(obj, key);
                    free(key);
                    return js_is_undefined(val) ? JS_FALSE : JS_TRUE;
                }
                if (strcmp(method, "set") == 0 && argc >= 2) {
                    char* key = js_to_cstring(args[0]);
                    for (char* p = key; *p; p++) if (*p >= 'A' && *p <= 'Z') *p += 32;
                    js_object_set(obj, key, args[1]);
                    free(key);
                    return JS_UNDEFINED;
                }
                if (strcmp(method, "delete") == 0 && argc > 0) {
                    char* key = js_to_cstring(args[0]);
                    for (char* p = key; *p; p++) if (*p >= 'A' && *p <= 'Z') *p += 32;
                    // Tombstone the entry
                    unsigned int idx = hash_string(key) & (obj->capacity - 1);
                    unsigned int start = idx;
                    while (obj->entries[idx].occupied) {
                        if (obj->entries[idx].occupied == 1 && strcmp(obj->entries[idx].key, key) == 0) {
                            free(obj->entries[idx].key);
                            obj->entries[idx].key = NULL;
                            obj->entries[idx].occupied = 2;
                            obj->count--;
                            break;
                        }
                        idx = (idx + 1) & (obj->capacity - 1);
                        if (idx == start) break;
                    }
                    free(key);
                    return JS_UNDEFINED;
                }
                if (strcmp(method, "forEach") == 0 && argc > 0 && js_is_function(args[0])) {
                    JSFunction* fn = (JSFunction*)js_as_ptr(args[0]);
                    for (int i = 0; i < obj->capacity; i++) {
                        if (obj->entries[i].occupied == 1 && obj->entries[i].key[0] != '_') {
                            JSValue cb_args[2] = { obj->entries[i].value, js_string_from_cstr(obj->entries[i].key) };
                            fn->fn(cb_args, 2, fn->closure_env);
                        }
                    }
                    return JS_UNDEFINED;
                }
                if (strcmp(method, "entries") == 0) {
                    JSValue arr = js_array_new();
                    for (int i = 0; i < obj->capacity; i++) {
                        if (obj->entries[i].occupied == 1 && obj->entries[i].key[0] != '_') {
                            JSValue pair = js_array_new();
                            js_array_push_val(pair, js_string_from_cstr(obj->entries[i].key));
                            js_array_push_val(pair, obj->entries[i].value);
                            js_array_push_val(arr, pair);
                        }
                    }
                    return arr;
                }
                if (strcmp(method, "keys") == 0) {
                    JSValue arr = js_array_new();
                    for (int i = 0; i < obj->capacity; i++) {
                        if (obj->entries[i].occupied == 1 && obj->entries[i].key[0] != '_')
                            js_array_push_val(arr, js_string_from_cstr(obj->entries[i].key));
                    }
                    return arr;
                }
                if (strcmp(method, "values") == 0) {
                    JSValue arr = js_array_new();
                    for (int i = 0; i < obj->capacity; i++) {
                        if (obj->entries[i].occupied == 1 && obj->entries[i].key[0] != '_')
                            js_array_push_val(arr, obj->entries[i].value);
                    }
                    return arr;
                }
            }
        }

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

    // Function methods: bind, call, apply
    if (js_is_function(this_val)) {
        if (strcmp(method, "bind") == 0) {
            // fn.bind(thisArg) — return a new function that calls fn with bound this
            JSValue bind_this = (argc > 0) ? args[0] : JS_UNDEFINED;
            // Allocate closure env: [original_fn, bind_this]
            void* env = js_alloc_closure_env(2);
            ((JSValue*)env)[0] = this_val;
            ((JSValue*)env)[1] = bind_this;
            return js_func_new(js_bind_trampoline, env, 0);
        }
        if (strcmp(method, "call") == 0) {
            JSValue call_this = (argc > 0) ? args[0] : JS_UNDEFINED;
            JSFunction* fn = (JSFunction*)js_as_ptr(this_val);
            js_this_push(call_this);
            JSValue result = fn->fn(args + 1, argc > 0 ? argc - 1 : 0, fn->closure_env);
            js_this_pop();
            return result;
        }
        if (strcmp(method, "apply") == 0) {
            JSValue apply_this = (argc > 0) ? args[0] : JS_UNDEFINED;
            JSFunction* fn = (JSFunction*)js_as_ptr(this_val);
            js_this_push(apply_this);
            JSValue result;
            if (argc > 1 && js_is_array(args[1])) {
                JSArray* arr = (JSArray*)js_as_ptr(args[1]);
                result = fn->fn(arr->data, arr->length, fn->closure_env);
            } else {
                result = fn->fn(NULL, 0, fn->closure_env);
            }
            js_this_pop();
            return result;
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

