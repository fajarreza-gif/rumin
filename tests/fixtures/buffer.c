#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define BUFFER_MAX_CAPACITY 4096
#define STATUS_OK 0

typedef enum {
    BUFFER_UNINITIALIZED,
    BUFFER_READY,
    BUFFER_OVERFLOW
} BufferState;

typedef struct {
    char data[BUFFER_MAX_CAPACITY];
    size_t length;
    BufferState state;
} ByteBuffer;

int byte_buffer_init(ByteBuffer *buf) {
    if (!buf) return -1;
    buf->length = 0;
    buf->state = BUFFER_READY;
    return STATUS_OK;
}

int byte_buffer_append(ByteBuffer *buf, const char *src, size_t len) {
    if (buf->length + len > BUFFER_MAX_CAPACITY) {
        buf->state = BUFFER_OVERFLOW;
        return -1;
    }
    memcpy(buf->data + buf->length, src, len);
    buf->length += len;
    return STATUS_OK;
}
