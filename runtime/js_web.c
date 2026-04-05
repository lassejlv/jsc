// ============================================================
// Web Standard APIs: Request, Response, Headers, URL
// ============================================================

// --- Headers ---

JSValue js_headers_new(JSValue init) {
    JSValue h = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(h);
    js_object_set(obj, "__type", js_string_from_cstr("Headers"));

    // If init is an object, copy its entries (lowercasing keys)
    if (js_is_object(init)) {
        JSObject* src = (JSObject*)js_as_ptr(init);
        for (int i = 0; i < src->capacity; i++) {
            if (src->entries[i].occupied == 1) {
                char* key = _strdup(src->entries[i].key);
                for (char* p = key; *p; p++) if (*p >= 'A' && *p <= 'Z') *p += 32;
                js_object_set(obj, key, src->entries[i].value);
                free(key);
            }
        }
    }
    return h;
}

// --- URL ---

JSValue js_url_new(const char* url_str) {
    JSValue u = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(u);
    js_object_set(obj, "__type", js_string_from_cstr("URL"));
    js_object_set(obj, "href", js_string_from_cstr(url_str));

    // Parse protocol
    const char* proto_end = strstr(url_str, "://");
    if (proto_end) {
        int plen = (int)(proto_end - url_str) + 1; // include ':'
        char proto[32];
        snprintf(proto, sizeof(proto), "%.*s:", (int)(proto_end - url_str), url_str);
        js_object_set(obj, "protocol", js_string_from_cstr(proto));

        const char* after_proto = proto_end + 3;
        // Find end of host (: or / or ?)
        const char* host_end = after_proto;
        while (*host_end && *host_end != '/' && *host_end != '?' && *host_end != '#') host_end++;

        char host[256];
        int hlen = (int)(host_end - after_proto);
        if (hlen > 255) hlen = 255;
        memcpy(host, after_proto, hlen);
        host[hlen] = '\0';
        js_object_set(obj, "host", js_string_from_cstr(host));

        // Split host:port
        char* colon = strchr(host, ':');
        if (colon) {
            *colon = '\0';
            js_object_set(obj, "hostname", js_string_from_cstr(host));
            js_object_set(obj, "port", js_string_from_cstr(colon + 1));
        } else {
            js_object_set(obj, "hostname", js_string_from_cstr(host));
            js_object_set(obj, "port", js_string_from_cstr(""));
        }

        // Origin
        char origin[512];
        snprintf(origin, sizeof(origin), "%.*s://%s", (int)(proto_end - url_str), url_str, host);
        js_object_set(obj, "origin", js_string_from_cstr(origin));

        // Pathname, search, hash
        if (*host_end == '/') {
            const char* path_start = host_end;
            const char* query = strchr(path_start, '?');
            const char* hash = strchr(path_start, '#');
            int path_end_offset = query ? (int)(query - path_start) : (hash ? (int)(hash - path_start) : (int)strlen(path_start));
            char pathname[2048];
            if (path_end_offset > 2047) path_end_offset = 2047;
            memcpy(pathname, path_start, path_end_offset);
            pathname[path_end_offset] = '\0';
            js_object_set(obj, "pathname", js_string_from_cstr(pathname));

            if (query) {
                const char* search_end = hash ? hash : query + strlen(query);
                char search[4096];
                int slen = (int)(search_end - query);
                if (slen > 4095) slen = 4095;
                memcpy(search, query, slen);
                search[slen] = '\0';
                js_object_set(obj, "search", js_string_from_cstr(search));
            } else {
                js_object_set(obj, "search", js_string_from_cstr(""));
            }

            if (hash) {
                js_object_set(obj, "hash", js_string_from_cstr(hash));
            } else {
                js_object_set(obj, "hash", js_string_from_cstr(""));
            }
        } else {
            js_object_set(obj, "pathname", js_string_from_cstr("/"));
            if (*host_end == '?') {
                js_object_set(obj, "search", js_string_from_cstr(host_end));
            } else {
                js_object_set(obj, "search", js_string_from_cstr(""));
            }
            js_object_set(obj, "hash", js_string_from_cstr(""));
        }
    } else {
        // Relative URL / path only
        js_object_set(obj, "protocol", js_string_from_cstr(""));
        js_object_set(obj, "host", js_string_from_cstr(""));
        js_object_set(obj, "hostname", js_string_from_cstr(""));
        js_object_set(obj, "port", js_string_from_cstr(""));
        js_object_set(obj, "origin", js_string_from_cstr(""));

        const char* query = strchr(url_str, '?');
        if (query) {
            char pathname[2048];
            int plen = (int)(query - url_str);
            if (plen > 2047) plen = 2047;
            memcpy(pathname, url_str, plen);
            pathname[plen] = '\0';
            js_object_set(obj, "pathname", js_string_from_cstr(pathname));
            js_object_set(obj, "search", js_string_from_cstr(query));
        } else {
            js_object_set(obj, "pathname", js_string_from_cstr(url_str));
            js_object_set(obj, "search", js_string_from_cstr(""));
        }
        js_object_set(obj, "hash", js_string_from_cstr(""));
    }

    return u;
}

// JSValue wrapper for js_url_new
JSValue js_url_new_val(JSValue url_val) {
    char* s = js_to_cstring(url_val);
    JSValue r = js_url_new(s);
    free(s);
    return r;
}

// --- Request ---

JSValue js_request_new(JSValue url_val, JSValue init_val) {
    JSValue req = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(req);
    js_object_set(obj, "__type", js_string_from_cstr("Request"));

    char* url = js_to_cstring(url_val);
    js_object_set(obj, "url", js_string_from_cstr(url));
    js_object_set(obj, "method", js_string_from_cstr("GET"));
    js_object_set(obj, "headers", js_headers_new(JS_UNDEFINED));
    js_object_set(obj, "__body", js_string_from_cstr(""));

    // Parse URL
    JSValue parsed_url = js_url_new(url);
    js_object_set(obj, "parsedUrl", parsed_url);
    free(url);

    // Override from init
    if (js_is_object(init_val)) {
        JSObject* init = (JSObject*)js_as_ptr(init_val);
        JSValue method_val = js_object_get(init, "method");
        if (js_is_string(method_val)) {
            js_object_set(obj, "method", method_val);
        }
        JSValue headers_val = js_object_get(init, "headers");
        if (js_is_object(headers_val)) {
            js_object_set(obj, "headers", js_headers_new(headers_val));
        }
        JSValue body_val = js_object_get(init, "body");
        if (!js_is_undefined(body_val)) {
            js_object_set(obj, "__body", body_val);
        }
    }

    return req;
}

// --- Response construction ---

JSValue js_response_new(JSValue body_val, JSValue init_val) {
    JSValue resp = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(resp);
    js_object_set(obj, "__type", js_string_from_cstr("Response"));

    // Body
    if (js_is_string(body_val)) {
        js_object_set(obj, "__body", body_val);
    } else if (js_is_null(body_val) || js_is_undefined(body_val)) {
        js_object_set(obj, "__body", js_string_from_cstr(""));
    } else {
        // Convert to string
        js_object_set(obj, "__body", js_to_string_val(body_val));
    }

    // Defaults
    int status = 200;
    JSValue headers = js_headers_new(JS_UNDEFINED);

    if (js_is_object(init_val)) {
        JSObject* init = (JSObject*)js_as_ptr(init_val);
        JSValue status_val = js_object_get(init, "status");
        if (js_is_number(status_val)) status = (int)js_as_number(status_val);
        JSValue headers_val = js_object_get(init, "headers");
        if (js_is_object(headers_val)) headers = js_headers_new(headers_val);
        JSValue statusText_val = js_object_get(init, "statusText");
        if (js_is_string(statusText_val)) {
            js_object_set(obj, "statusText", statusText_val);
        }
    }

    js_object_set(obj, "status", js_number((double)status));
    js_object_set(obj, "ok", (status >= 200 && status < 300) ? JS_TRUE : JS_FALSE);
    js_object_set(obj, "headers", headers);
    if (js_is_undefined(js_object_get(obj, "statusText"))) {
        js_object_set(obj, "statusText", js_string_from_cstr(status == 200 ? "OK" : ""));
    }

    return resp;
}

// Response.json(data, init) — static method
JSValue js_response_json(JSValue data, JSValue init_val) {
    JSValue body = js_json_stringify(data);
    JSValue resp = js_response_new(body, init_val);
    JSObject* obj = (JSObject*)js_as_ptr(resp);
    // Set Content-Type header
    JSValue headers = js_object_get(obj, "headers");
    if (js_is_object(headers)) {
        js_object_set((JSObject*)js_as_ptr(headers), "content-type",
            js_string_from_cstr("application/json"));
    }
    return resp;
}

// Response.redirect(url, status)
JSValue js_response_redirect(JSValue url_val, JSValue status_val) {
    int status = js_is_number(status_val) ? (int)js_as_number(status_val) : 302;
    JSValue resp = js_response_new(js_string_from_cstr(""), JS_UNDEFINED);
    JSObject* obj = (JSObject*)js_as_ptr(resp);
    js_object_set(obj, "status", js_number((double)status));
    js_object_set(obj, "ok", JS_FALSE);
    JSValue headers = js_object_get(obj, "headers");
    if (js_is_object(headers)) {
        char* url = js_to_cstring(url_val);
        js_object_set((JSObject*)js_as_ptr(headers), "location", js_string_from_cstr(url));
        free(url);
    }
    return resp;
}
