// ============================================================
// Runtime init
// ============================================================

// Allocate closure environment (array of JSValue)
void* js_alloc_closure_env(int count) {
    return calloc(count, sizeof(JSValue));
}

static int js_curl_initialized = 0;

void js_runtime_init(void) {
    srand((unsigned int)time(NULL));
    js_rand_seeded = 1;
    if (!js_curl_initialized) {
        curl_global_init(CURL_GLOBAL_DEFAULT);
        js_curl_initialized = 1;
    }
}
