#ifndef SYNAPSE_CDR_H
#define SYNAPSE_CDR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SYNAPSE_CDR_LE_ENCAPSULATION_SIZE 4U

typedef enum synapse_cdr_result {
    SYNAPSE_CDR_OK = 0,
    SYNAPSE_CDR_NULL_ARGUMENT = -1,
    SYNAPSE_CDR_BUFFER_TOO_SMALL = -2,
    SYNAPSE_CDR_INVALID_ENCAPSULATION = -3,
    SYNAPSE_CDR_TRAILING_BYTES = -4,
    SYNAPSE_CDR_SIZE_MISMATCH = -5
} synapse_cdr_result_t;

typedef struct synapse_cdr_writer {
    uint8_t *bytes;
    size_t capacity;
    size_t position;
    size_t alignment_origin;
    synapse_cdr_result_t result;
} synapse_cdr_writer_t;

typedef struct synapse_cdr_reader {
    const uint8_t *bytes;
    size_t size;
    size_t position;
    size_t alignment_origin;
    synapse_cdr_result_t result;
} synapse_cdr_reader_t;

synapse_cdr_result_t synapse_cdr_writer_init_le(
    synapse_cdr_writer_t *writer, uint8_t *bytes, size_t capacity);
synapse_cdr_result_t synapse_cdr_write_u8(
    synapse_cdr_writer_t *writer, uint8_t value);
synapse_cdr_result_t synapse_cdr_write_i16(
    synapse_cdr_writer_t *writer, int16_t value);
synapse_cdr_result_t synapse_cdr_write_u16(
    synapse_cdr_writer_t *writer, uint16_t value);
synapse_cdr_result_t synapse_cdr_write_i32(
    synapse_cdr_writer_t *writer, int32_t value);
synapse_cdr_result_t synapse_cdr_write_u32(
    synapse_cdr_writer_t *writer, uint32_t value);
synapse_cdr_result_t synapse_cdr_write_u64(
    synapse_cdr_writer_t *writer, uint64_t value);
synapse_cdr_result_t synapse_cdr_write_f32(
    synapse_cdr_writer_t *writer, float value);
synapse_cdr_result_t synapse_cdr_writer_finish_exact(
    synapse_cdr_writer_t *writer, size_t expected_size, size_t *written);

synapse_cdr_result_t synapse_cdr_reader_init_le(
    synapse_cdr_reader_t *reader, const uint8_t *bytes, size_t size);
synapse_cdr_result_t synapse_cdr_read_u8(
    synapse_cdr_reader_t *reader, uint8_t *value);
synapse_cdr_result_t synapse_cdr_read_i16(
    synapse_cdr_reader_t *reader, int16_t *value);
synapse_cdr_result_t synapse_cdr_read_u16(
    synapse_cdr_reader_t *reader, uint16_t *value);
synapse_cdr_result_t synapse_cdr_read_i32(
    synapse_cdr_reader_t *reader, int32_t *value);
synapse_cdr_result_t synapse_cdr_read_u32(
    synapse_cdr_reader_t *reader, uint32_t *value);
synapse_cdr_result_t synapse_cdr_read_u64(
    synapse_cdr_reader_t *reader, uint64_t *value);
synapse_cdr_result_t synapse_cdr_read_f32(
    synapse_cdr_reader_t *reader, float *value);
synapse_cdr_result_t synapse_cdr_reader_finish(
    synapse_cdr_reader_t *reader);

#ifdef __cplusplus
}
#endif

#endif
