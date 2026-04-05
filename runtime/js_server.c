// ============================================================
// HTTP Server — POSIX sockets
// ============================================================

#ifndef _WIN32
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#endif

#define SERVER_BUF_SIZE 65536

static JSValue parse_http_request(const char* raw, int len, int port) {
    // Parse "METHOD /path HTTP/1.1\r\n"
    const char* method_end = memchr(raw, ' ', len);
    if (!method_end) return JS_UNDEFINED;

    char method[16];
    int mlen = (int)(method_end - raw);
    if (mlen > 15) mlen = 15;
    memcpy(method, raw, mlen);
    method[mlen] = '\0';

    const char* path_start = method_end + 1;
    const char* path_end = memchr(path_start, ' ', len - (path_start - raw));
    if (!path_end) return JS_UNDEFINED;

    char path[4096];
    int plen = (int)(path_end - path_start);
    if (plen > 4095) plen = 4095;
    memcpy(path, path_start, plen);
    path[plen] = '\0';

    // Build full URL
    char url[4196];
    snprintf(url, sizeof(url), "http://localhost:%d%s", port, path);

    // Parse headers
    JSValue headers = js_headers_new(JS_UNDEFINED);
    const char* header_start = strstr(raw, "\r\n");
    if (header_start) {
        header_start += 2;
        while (header_start < raw + len) {
            if (header_start[0] == '\r' && header_start[1] == '\n') break; // end of headers
            const char* colon = memchr(header_start, ':', raw + len - header_start);
            if (!colon) break;

            // Key
            int klen = (int)(colon - header_start);
            char* key = (char*)malloc(klen + 1);
            memcpy(key, header_start, klen);
            key[klen] = '\0';
            for (int i = 0; i < klen; i++) if (key[i] >= 'A' && key[i] <= 'Z') key[i] += 32;

            // Value
            const char* val_start = colon + 1;
            while (*val_start == ' ') val_start++;
            const char* line_end = strstr(val_start, "\r\n");
            if (!line_end) line_end = raw + len;
            int vlen = (int)(line_end - val_start);

            js_object_set((JSObject*)js_as_ptr(headers), key, js_string_from_len(val_start, vlen));
            free(key);

            header_start = line_end + 2;
        }
    }

    // Body (after \r\n\r\n)
    JSValue body = js_string_from_cstr("");
    const char* body_start = strstr(raw, "\r\n\r\n");
    if (body_start) {
        body_start += 4;
        int body_len = len - (int)(body_start - raw);
        if (body_len > 0) {
            body = js_string_from_len(body_start, body_len);
        }
    }

    // Create Request object
    JSValue req = js_object_new();
    JSObject* obj = (JSObject*)js_as_ptr(req);
    js_object_set(obj, "__type", js_string_from_cstr("Request"));
    js_object_set(obj, "method", js_string_from_cstr(method));
    js_object_set(obj, "url", js_string_from_cstr(url));
    js_object_set(obj, "headers", headers);
    js_object_set(obj, "__body", body);

    // Parse URL for convenient access
    JSValue parsed = js_url_new(url);
    JSObject* purl = (JSObject*)js_as_ptr(parsed);
    js_object_set(obj, "pathname", js_object_get(purl, "pathname"));
    js_object_set(obj, "search", js_object_get(purl, "search"));

    return req;
}

static void send_http_response(int fd, JSValue response) {
    if (!js_is_object(response)) {
        const char* fallback = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 21\r\nConnection: close\r\n\r\nInternal Server Error";
        write(fd, fallback, strlen(fallback));
        return;
    }

    JSObject* obj = (JSObject*)js_as_ptr(response);
    int status = 200;
    JSValue status_val = js_object_get(obj, "status");
    if (js_is_number(status_val)) status = (int)js_as_number(status_val);

    char* status_text = "OK";
    JSValue st_val = js_object_get(obj, "statusText");
    char* st_str = NULL;
    if (js_is_string(st_val)) { st_str = js_to_cstring(st_val); status_text = st_str; }

    // Get body
    JSValue body_val = js_object_get(obj, "__body");
    char* body = js_to_cstring(body_val);
    int body_len = (int)strlen(body);

    // Build response
    char header_buf[8192];
    int hlen = snprintf(header_buf, sizeof(header_buf),
        "HTTP/1.1 %d %s\r\nContent-Length: %d\r\nConnection: close\r\n",
        status, status_text, body_len);

    // Write response headers from the Response object
    JSValue headers = js_object_get(obj, "headers");
    if (js_is_object(headers)) {
        JSObject* hdr = (JSObject*)js_as_ptr(headers);
        for (int i = 0; i < hdr->capacity; i++) {
            if (hdr->entries[i].occupied == 1 && hdr->entries[i].key[0] != '_') {
                char* hv = js_to_cstring(hdr->entries[i].value);
                hlen += snprintf(header_buf + hlen, sizeof(header_buf) - hlen,
                    "%s: %s\r\n", hdr->entries[i].key, hv);
                free(hv);
            }
        }
    }
    hlen += snprintf(header_buf + hlen, sizeof(header_buf) - hlen, "\r\n");

    write(fd, header_buf, hlen);
    if (body_len > 0) write(fd, body, body_len);

    free(body);
    if (st_str) free(st_str);
}

JSValue js_serve(JSValue config) {
#ifdef _WIN32
    fprintf(stderr, "Error: JSC.serve() is not supported on Windows yet\n");
    exit(1);
#else
    if (!js_is_object(config)) {
        js_throw(js_string_from_cstr("TypeError: JSC.serve() expects a config object"));
        return JS_UNDEFINED;
    }

    JSObject* cfg = (JSObject*)js_as_ptr(config);
    int port = 3000;
    JSValue port_val = js_object_get(cfg, "port");
    if (js_is_number(port_val)) port = (int)js_as_number(port_val);

    JSValue fetch_handler = js_object_get(cfg, "fetch");
    if (!js_is_function(fetch_handler)) {
        js_throw(js_string_from_cstr("TypeError: JSC.serve() config must have a fetch function"));
        return JS_UNDEFINED;
    }

    // Create socket
    int server_fd = socket(AF_INET, SOCK_STREAM, 0);
    if (server_fd < 0) {
        js_throw(js_string_from_cstr("Error: Failed to create socket"));
        return JS_UNDEFINED;
    }

    int opt = 1;
    setsockopt(server_fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));

    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = INADDR_ANY;
    addr.sin_port = htons(port);

    if (bind(server_fd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        close(server_fd);
        char msg[128];
        snprintf(msg, sizeof(msg), "Error: Failed to bind to port %d", port);
        js_throw(js_string_from_cstr(msg));
        return JS_UNDEFINED;
    }

    if (listen(server_fd, 128) < 0) {
        close(server_fd);
        js_throw(js_string_from_cstr("Error: Failed to listen"));
        return JS_UNDEFINED;
    }

    printf("Listening on http://localhost:%d\n", port);
    fflush(stdout);

    // Accept loop
    while (1) {
        struct sockaddr_in client_addr;
        socklen_t client_len = sizeof(client_addr);
        int client_fd = accept(server_fd, (struct sockaddr*)&client_addr, &client_len);
        if (client_fd < 0) continue;

        // Read request
        char buf[SERVER_BUF_SIZE];
        int n = (int)read(client_fd, buf, sizeof(buf) - 1);
        if (n <= 0) { close(client_fd); continue; }
        buf[n] = '\0';

        // Parse request
        JSValue request = parse_http_request(buf, n, port);
        if (js_is_undefined(request)) {
            close(client_fd);
            continue;
        }

        // Call fetch handler
        JSFunction* fn = (JSFunction*)js_as_ptr(fetch_handler);
        JSValue args[1] = { request };
        JSValue response;

        // try/catch around handler
        void* try_buf = js_try_get_buf();
        if (_setjmp(try_buf) == 0) {
            response = fn->fn(args, 1, fn->closure_env);
            js_try_exit();

            // If response is a Promise, await it
            if (js_is_object(response)) {
                JSValue rtype = js_object_get((JSObject*)js_as_ptr(response), "__type");
                if (js_is_string(rtype) && strcmp(js_as_string(rtype)->data, "Promise") == 0) {
                    response = js_await(response);
                }
            }
        } else {
            // Handler threw an error
            JSValue err = js_get_error();
            char* err_str = js_to_cstring(err);
            fprintf(stderr, "Request handler error: %s\n", err_str);
            free(err_str);
            response = js_response_new(
                js_string_from_cstr("Internal Server Error"),
                JS_UNDEFINED
            );
            js_object_set((JSObject*)js_as_ptr(response), "status", js_number(500));
        }

        // Send response
        send_http_response(client_fd, response);
        close(client_fd);
    }

    close(server_fd);
    return JS_UNDEFINED;
#endif
}
