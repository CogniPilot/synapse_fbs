#include <synapse/cdr.h>

#include <string.h>

_Static_assert(sizeof(float) == sizeof(uint32_t),
               "CDRv1 float requires a 32-bit float representation");

static synapse_cdr_result_t writer_fail(
    synapse_cdr_writer_t *writer, synapse_cdr_result_t result) {
    if (writer != NULL && writer->result == SYNAPSE_CDR_OK) {
        writer->result = result;
    }
    return result;
}

static synapse_cdr_result_t reader_fail(
    synapse_cdr_reader_t *reader, synapse_cdr_result_t result) {
    if (reader != NULL && reader->result == SYNAPSE_CDR_OK) {
        reader->result = result;
    }
    return result;
}

static synapse_cdr_result_t writer_reserve(
    synapse_cdr_writer_t *writer, size_t alignment, size_t size) {
    size_t relative;
    size_t padding;

    if (writer == NULL) {
        return SYNAPSE_CDR_NULL_ARGUMENT;
    }
    if (writer->result != SYNAPSE_CDR_OK) {
        return writer->result;
    }
    relative = writer->position - writer->alignment_origin;
    padding = (alignment - (relative & (alignment - 1U))) & (alignment - 1U);
    if (padding > writer->capacity - writer->position ||
        size > writer->capacity - writer->position - padding) {
        return writer_fail(writer, SYNAPSE_CDR_BUFFER_TOO_SMALL);
    }
    memset(writer->bytes + writer->position, 0, padding);
    writer->position += padding;
    return SYNAPSE_CDR_OK;
}

static synapse_cdr_result_t reader_reserve(
    synapse_cdr_reader_t *reader, size_t alignment, size_t size) {
    size_t relative;
    size_t padding;

    if (reader == NULL) {
        return SYNAPSE_CDR_NULL_ARGUMENT;
    }
    if (reader->result != SYNAPSE_CDR_OK) {
        return reader->result;
    }
    relative = reader->position - reader->alignment_origin;
    padding = (alignment - (relative & (alignment - 1U))) & (alignment - 1U);
    if (padding > reader->size - reader->position ||
        size > reader->size - reader->position - padding) {
        return reader_fail(reader, SYNAPSE_CDR_BUFFER_TOO_SMALL);
    }
    reader->position += padding;
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_writer_init_le(
    synapse_cdr_writer_t *writer, uint8_t *bytes, size_t capacity) {
    static const uint8_t encapsulation[SYNAPSE_CDR_LE_ENCAPSULATION_SIZE] = {
        0x00U, 0x01U, 0x00U, 0x00U};

    if (writer == NULL || bytes == NULL) {
        return SYNAPSE_CDR_NULL_ARGUMENT;
    }
    writer->bytes = bytes;
    writer->capacity = capacity;
    writer->position = 0U;
    writer->alignment_origin = SYNAPSE_CDR_LE_ENCAPSULATION_SIZE;
    writer->result = SYNAPSE_CDR_OK;
    if (capacity < sizeof(encapsulation)) {
        return writer_fail(writer, SYNAPSE_CDR_BUFFER_TOO_SMALL);
    }
    memcpy(bytes, encapsulation, sizeof(encapsulation));
    writer->position = sizeof(encapsulation);
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_write_u8(
    synapse_cdr_writer_t *writer, uint8_t value) {
    if (writer_reserve(writer, 1U, 1U) != SYNAPSE_CDR_OK) {
        return writer == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : writer->result;
    }
    writer->bytes[writer->position++] = value;
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_write_u16(
    synapse_cdr_writer_t *writer, uint16_t value) {
    if (writer_reserve(writer, 2U, 2U) != SYNAPSE_CDR_OK) {
        return writer == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : writer->result;
    }
    writer->bytes[writer->position++] = (uint8_t)value;
    writer->bytes[writer->position++] = (uint8_t)(value >> 8U);
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_write_i16(
    synapse_cdr_writer_t *writer, int16_t value) {
    uint16_t bits;
    memcpy(&bits, &value, sizeof(bits));
    return synapse_cdr_write_u16(writer, bits);
}

synapse_cdr_result_t synapse_cdr_write_u32(
    synapse_cdr_writer_t *writer, uint32_t value) {
    unsigned int shift;
    if (writer_reserve(writer, 4U, 4U) != SYNAPSE_CDR_OK) {
        return writer == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : writer->result;
    }
    for (shift = 0U; shift < 32U; shift += 8U) {
        writer->bytes[writer->position++] = (uint8_t)(value >> shift);
    }
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_write_i32(
    synapse_cdr_writer_t *writer, int32_t value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof(bits));
    return synapse_cdr_write_u32(writer, bits);
}

synapse_cdr_result_t synapse_cdr_write_u64(
    synapse_cdr_writer_t *writer, uint64_t value) {
    unsigned int shift;
    if (writer_reserve(writer, 8U, 8U) != SYNAPSE_CDR_OK) {
        return writer == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : writer->result;
    }
    for (shift = 0U; shift < 64U; shift += 8U) {
        writer->bytes[writer->position++] = (uint8_t)(value >> shift);
    }
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_write_f32(
    synapse_cdr_writer_t *writer, float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof(bits));
    return synapse_cdr_write_u32(writer, bits);
}

synapse_cdr_result_t synapse_cdr_writer_finish_exact(
    synapse_cdr_writer_t *writer, size_t expected_size, size_t *written) {
    if (writer == NULL) {
        return SYNAPSE_CDR_NULL_ARGUMENT;
    }
    if (writer->result != SYNAPSE_CDR_OK) {
        return writer->result;
    }
    if (writer->position != expected_size) {
        return writer_fail(writer, SYNAPSE_CDR_SIZE_MISMATCH);
    }
    if (written != NULL) {
        *written = writer->position;
    }
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_reader_init_le(
    synapse_cdr_reader_t *reader, const uint8_t *bytes, size_t size) {
    static const uint8_t encapsulation[SYNAPSE_CDR_LE_ENCAPSULATION_SIZE] = {
        0x00U, 0x01U, 0x00U, 0x00U};

    if (reader == NULL || bytes == NULL) {
        return SYNAPSE_CDR_NULL_ARGUMENT;
    }
    reader->bytes = bytes;
    reader->size = size;
    reader->position = 0U;
    reader->alignment_origin = SYNAPSE_CDR_LE_ENCAPSULATION_SIZE;
    reader->result = SYNAPSE_CDR_OK;
    if (size < sizeof(encapsulation)) {
        return reader_fail(reader, SYNAPSE_CDR_BUFFER_TOO_SMALL);
    }
    if (memcmp(bytes, encapsulation, sizeof(encapsulation)) != 0) {
        return reader_fail(reader, SYNAPSE_CDR_INVALID_ENCAPSULATION);
    }
    reader->position = sizeof(encapsulation);
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_read_u8(
    synapse_cdr_reader_t *reader, uint8_t *value) {
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    if (reader_reserve(reader, 1U, 1U) != SYNAPSE_CDR_OK) {
        return reader == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : reader->result;
    }
    *value = reader->bytes[reader->position++];
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_read_u16(
    synapse_cdr_reader_t *reader, uint16_t *value) {
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    if (reader_reserve(reader, 2U, 2U) != SYNAPSE_CDR_OK) {
        return reader == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : reader->result;
    }
    *value = (uint16_t)reader->bytes[reader->position] |
             ((uint16_t)reader->bytes[reader->position + 1U] << 8U);
    reader->position += 2U;
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_read_i16(
    synapse_cdr_reader_t *reader, int16_t *value) {
    uint16_t bits;
    synapse_cdr_result_t result;
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    result = synapse_cdr_read_u16(reader, &bits);
    if (result == SYNAPSE_CDR_OK) {
        memcpy(value, &bits, sizeof(bits));
    }
    return result;
}

synapse_cdr_result_t synapse_cdr_read_u32(
    synapse_cdr_reader_t *reader, uint32_t *value) {
    unsigned int shift;
    uint32_t bits = 0U;
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    if (reader_reserve(reader, 4U, 4U) != SYNAPSE_CDR_OK) {
        return reader == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : reader->result;
    }
    for (shift = 0U; shift < 32U; shift += 8U) {
        bits |= (uint32_t)reader->bytes[reader->position++] << shift;
    }
    *value = bits;
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_read_i32(
    synapse_cdr_reader_t *reader, int32_t *value) {
    uint32_t bits;
    synapse_cdr_result_t result;
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    result = synapse_cdr_read_u32(reader, &bits);
    if (result == SYNAPSE_CDR_OK) {
        memcpy(value, &bits, sizeof(bits));
    }
    return result;
}

synapse_cdr_result_t synapse_cdr_read_u64(
    synapse_cdr_reader_t *reader, uint64_t *value) {
    unsigned int shift;
    uint64_t bits = 0U;
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    if (reader_reserve(reader, 8U, 8U) != SYNAPSE_CDR_OK) {
        return reader == NULL ? SYNAPSE_CDR_NULL_ARGUMENT : reader->result;
    }
    for (shift = 0U; shift < 64U; shift += 8U) {
        bits |= (uint64_t)reader->bytes[reader->position++] << shift;
    }
    *value = bits;
    return SYNAPSE_CDR_OK;
}

synapse_cdr_result_t synapse_cdr_read_f32(
    synapse_cdr_reader_t *reader, float *value) {
    uint32_t bits;
    synapse_cdr_result_t result;
    if (value == NULL) {
        return reader_fail(reader, SYNAPSE_CDR_NULL_ARGUMENT);
    }
    result = synapse_cdr_read_u32(reader, &bits);
    if (result == SYNAPSE_CDR_OK) {
        memcpy(value, &bits, sizeof(bits));
    }
    return result;
}

synapse_cdr_result_t synapse_cdr_reader_finish(
    synapse_cdr_reader_t *reader) {
    if (reader == NULL) {
        return SYNAPSE_CDR_NULL_ARGUMENT;
    }
    if (reader->result != SYNAPSE_CDR_OK) {
        return reader->result;
    }
    if (reader->position != reader->size) {
        return reader_fail(reader, SYNAPSE_CDR_TRAILING_BYTES);
    }
    return SYNAPSE_CDR_OK;
}
