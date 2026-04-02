// ============================================================
// fetch() — synchronous HTTP client via libcurl
// ============================================================

typedef struct {
    char* data;
    size_t len;
    size_t cap;
} FetchBuffer;

static size_t fetch_write_cb(void* ptr, size_t size, size_t nmemb, void* userdata) {
    FetchBuffer* buf = (FetchBuffer*)userdata;
    size_t total = size * nmemb;
    while (buf->len + total >= buf->cap) {
        buf->cap *= 2;
        buf->data = (char*)realloc(buf->data, buf->cap);
    }
    memcpy(buf->data + buf->len, ptr, total);
    buf->len += total;
    buf->data[buf->len] = '\0';
    return total;
}

typedef struct {
    JSValue obj;           // JS object to store headers in
    char status_text[128]; // captures the reason phrase from HTTP/x.x NNN <reason>
} FetchHeaderCtx;

static size_t fetch_header_cb(char* buffer, size_t size, size_t nitems, void* userdata) {
    FetchHeaderCtx* ctx = (FetchHeaderCtx*)userdata;
    size_t total = size * nitems;

    // Capture status text from "HTTP/x.x NNN Reason Phrase\r\n"
    if (total > 9 && (memcmp(buffer, "HTTP/1.", 7) == 0 || memcmp(buffer, "HTTP/2", 6) == 0)) {
        char* p = memchr(buffer, ' ', total);
        if (p) {
            p++;
            int code = atoi(p);
            char* p2 = memchr(p, ' ', total - (p - buffer));
            int has_reason = 0;
            if (p2) {
                p2++;
                size_t reason_len = total - (p2 - buffer);
                while (reason_len > 0 && (p2[reason_len-1] == '\r' || p2[reason_len-1] == '\n' || p2[reason_len-1] == ' ')) reason_len--;
                if (reason_len > 0) {
                    if (reason_len > 127) reason_len = 127;
                    memcpy(ctx->status_text, p2, reason_len);
                    ctx->status_text[reason_len] = '\0';
                    has_reason = 1;
                }
            }
            if (!has_reason) {
                // HTTP/2 has no reason phrase — use standard ones
                const char* reason = "";
                switch (code) {
                    case 200: reason = "OK"; break;
                    case 201: reason = "Created"; break;
                    case 204: reason = "No Content"; break;
                    case 301: reason = "Moved Permanently"; break;
                    case 302: reason = "Found"; break;
                    case 304: reason = "Not Modified"; break;
                    case 400: reason = "Bad Request"; break;
                    case 401: reason = "Unauthorized"; break;
                    case 403: reason = "Forbidden"; break;
                    case 404: reason = "Not Found"; break;
                    case 405: reason = "Method Not Allowed"; break;
                    case 409: reason = "Conflict"; break;
                    case 422: reason = "Unprocessable Entity"; break;
                    case 429: reason = "Too Many Requests"; break;
                    case 500: reason = "Internal Server Error"; break;
                    case 502: reason = "Bad Gateway"; break;
                    case 503: reason = "Service Unavailable"; break;
                    case 504: reason = "Gateway Timeout"; break;
                }
                snprintf(ctx->status_text, sizeof(ctx->status_text), "%s", reason);
            }
        }
        return total;
    }

    // Parse "Key: Value\r\n" headers
    char* colon = memchr(buffer, ':', total);
    if (colon && ctx->obj) {
        size_t key_len = colon - buffer;
        char* key = (char*)malloc(key_len + 1);
        memcpy(key, buffer, key_len);
        key[key_len] = '\0';
        // Lowercase the key
        for (size_t i = 0; i < key_len; i++) {
            if (key[i] >= 'A' && key[i] <= 'Z') key[i] += 32;
        }
        // Skip ": " and trim trailing \r\n
        char* val_start = colon + 1;
        while (*val_start == ' ') val_start++;
        size_t val_len = total - (val_start - buffer);
        while (val_len > 0 && (val_start[val_len-1] == '\r' || val_start[val_len-1] == '\n')) val_len--;
        JSValue val = js_string_from_len(val_start, (int)val_len);
        js_object_set((JSObject*)js_as_ptr(ctx->obj), key, val);
        free(key);
    }
    return total;
}

JSValue js_fetch(JSValue url_val, JSValue options_val) {
    char* url = js_to_cstring(url_val);

    // Parse options
    char* method = _strdup("GET");
    char* body = NULL;
    struct curl_slist* headers_list = NULL;
    long timeout = 30;
    int follow_redirects = 1; // "follow" by default

    if (js_is_object(options_val)) {
        JSObject* opts = (JSObject*)js_as_ptr(options_val);

        // Method
        JSValue method_val = js_object_get(opts, "method");
        if (js_is_string(method_val)) {
            free(method);
            method = js_to_cstring(method_val);
            for (char* p = method; *p; p++) {
                if (*p >= 'a' && *p <= 'z') *p -= 32;
            }
        }

        // Body
        JSValue body_val = js_object_get(opts, "body");
        if (js_is_string(body_val)) {
            body = js_to_cstring(body_val);
        }

        // Headers
        JSValue hdrs = js_object_get(opts, "headers");
        if (js_is_object(hdrs)) {
            JSObject* hdr_obj = (JSObject*)js_as_ptr(hdrs);
            for (int i = 0; i < hdr_obj->capacity; i++) {
                if (hdr_obj->entries[i].occupied == 1) {
                    char* hv = js_to_cstring(hdr_obj->entries[i].value);
                    size_t line_len = strlen(hdr_obj->entries[i].key) + 2 + strlen(hv) + 1;
                    char* line = (char*)malloc(line_len);
                    snprintf(line, line_len, "%s: %s", hdr_obj->entries[i].key, hv);
                    headers_list = curl_slist_append(headers_list, line);
                    free(line);
                    free(hv);
                }
            }
        }

        // Timeout (non-standard but practical)
        JSValue timeout_val = js_object_get(opts, "timeout");
        if (js_is_number(timeout_val)) {
            double t = js_as_number(timeout_val);
            timeout = (long)(t / 1000.0); // ms to seconds
            if (timeout < 1) timeout = 1;
        }

        // Redirect control: "follow" (default), "manual", "error"
        JSValue redirect_val = js_object_get(opts, "redirect");
        if (js_is_string(redirect_val)) {
            char* redir = js_to_cstring(redirect_val);
            if (strcmp(redir, "manual") == 0) follow_redirects = 0;
            else if (strcmp(redir, "error") == 0) follow_redirects = -1; // will throw on redirect
            free(redir);
        }
    }

    // Perform request
    CURL* curl = curl_easy_init();
    if (!curl) {
        free(url); free(method);
        if (body) free(body);
        if (headers_list) curl_slist_free_all(headers_list);
        js_throw(js_string_from_cstr("TypeError: Failed to initialize HTTP request"));
        return JS_UNDEFINED;
    }

    FetchBuffer resp_buf = { (char*)malloc(1024), 0, 1024 };
    resp_buf.data[0] = '\0';

    JSValue resp_headers_obj = js_object_new();
    FetchHeaderCtx hdr_ctx = { resp_headers_obj, "" };

    curl_easy_setopt(curl, CURLOPT_URL, url);
    curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, fetch_write_cb);
    curl_easy_setopt(curl, CURLOPT_WRITEDATA, &resp_buf);
    curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, fetch_header_cb);
    curl_easy_setopt(curl, CURLOPT_HEADERDATA, &hdr_ctx);
    curl_easy_setopt(curl, CURLOPT_TIMEOUT, timeout);
    curl_easy_setopt(curl, CURLOPT_USERAGENT, "js-compiler/0.1");

    if (follow_redirects > 0) {
        curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1L);
        curl_easy_setopt(curl, CURLOPT_MAXREDIRS, 20L);
    } else {
        curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 0L);
    }

    // HEAD
    if (strcmp(method, "HEAD") == 0) {
        curl_easy_setopt(curl, CURLOPT_NOBODY, 1L);
    }
    // POST
    else if (strcmp(method, "POST") == 0) {
        curl_easy_setopt(curl, CURLOPT_POST, 1L);
        if (body) {
            curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body);
            curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE, (long)strlen(body));
        }
    }
    // PUT, DELETE, PATCH, OPTIONS, etc.
    else if (strcmp(method, "GET") != 0) {
        curl_easy_setopt(curl, CURLOPT_CUSTOMREQUEST, method);
        if (body) {
            curl_easy_setopt(curl, CURLOPT_POSTFIELDS, body);
            curl_easy_setopt(curl, CURLOPT_POSTFIELDSIZE, (long)strlen(body));
        }
    }

    if (headers_list) {
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers_list);
    }

    CURLcode res = curl_easy_perform(curl);

    long status_code = 0;
    curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &status_code);

    // Get final URL (after redirects)
    char* effective_url = NULL;
    curl_easy_getinfo(curl, CURLINFO_EFFECTIVE_URL, &effective_url);
    char* final_url = effective_url ? _strdup(effective_url) : _strdup(url);

    // Check if redirected
    long redirect_count = 0;
    curl_easy_getinfo(curl, CURLINFO_REDIRECT_COUNT, &redirect_count);

    if (headers_list) curl_slist_free_all(headers_list);
    curl_easy_cleanup(curl);

    // Network errors throw TypeError (matches spec)
    if (res != CURLE_OK) {
        char err_msg[256];
        snprintf(err_msg, sizeof(err_msg), "TypeError: fetch failed - %s", curl_easy_strerror(res));
        free(url); free(method); free(final_url);
        free(resp_buf.data);
        if (body) free(body);
        js_throw(js_string_from_cstr(err_msg));
        return JS_UNDEFINED;
    }

    // redirect: "error" mode — throw on redirects
    if (follow_redirects < 0 && status_code >= 300 && status_code < 400) {
        free(url); free(method); free(final_url);
        free(resp_buf.data);
        if (body) free(body);
        js_throw(js_string_from_cstr("TypeError: fetch failed - unexpected redirect"));
        return JS_UNDEFINED;
    }

    // Build Response object
    JSValue response = js_object_new();
    JSObject* resp_obj = (JSObject*)js_as_ptr(response);

    js_object_set(resp_obj, "ok", (status_code >= 200 && status_code < 300) ? JS_TRUE : JS_FALSE);
    js_object_set(resp_obj, "status", js_number((double)status_code));
    js_object_set(resp_obj, "statusText", js_string_from_cstr(hdr_ctx.status_text));
    js_object_set(resp_obj, "url", js_string_from_cstr(final_url));
    js_object_set(resp_obj, "redirected", redirect_count > 0 ? JS_TRUE : JS_FALSE);
    js_object_set(resp_obj, "type", js_string_from_cstr("basic"));
    js_object_set(resp_obj, "__body", js_string_from_len(resp_buf.data, (int)resp_buf.len));
    js_object_set(resp_obj, "headers", resp_headers_obj);
    js_object_set(resp_obj, "__type", js_string_from_cstr("Response"));

    free(resp_buf.data);
    free(url);
    free(method);
    free(final_url);
    if (body) free(body);

    return response;
}
