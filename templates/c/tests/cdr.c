#include <assert.h>
#include <stdint.h>
#include <string.h>

#include <synapse/cdr_catalog.h>

static void test_optical_flow_velocity(void) {
    static const uint8_t expected[SYNAPSE_CDR_OPTICAL_FLOW_VELOCITY_TOTAL_BYTES] = {
        0x00U, 0x01U, 0x00U, 0x00U,
        0x08U, 0x07U, 0x06U, 0x05U, 0x04U, 0x03U, 0x02U, 0x01U,
        0x00U, 0x00U, 0x80U, 0x3fU,
        0x00U, 0x00U, 0x00U, 0xc0U,
        0x00U, 0x00U, 0x60U, 0x40U,
        0x00U, 0x00U, 0x80U, 0xbeU,
        0x00U, 0x00U, 0x00U, 0x3fU,
        0xaaU, 0x05U, 0x01U, 0x02U,
    };
    const synapse_cdr_optical_flow_velocity_t input = {
        .timestamp_ns = UINT64_C(0x0102030405060708),
        .velocity_flu_m_s = {1.0F, -2.0F},
        .distance_m = 3.5F,
        .roll_rad = -0.25F,
        .pitch_rad = 0.5F,
        .quality = 0xaaU,
        .flags = 0x05U,
        .time_status = 1U,
        .id = 2U,
    };
    synapse_cdr_optical_flow_velocity_t output = {0};
    uint8_t bytes[sizeof(expected) + 1U];
    size_t written = 0U;

    assert(synapse_cdr_encode_optical_flow_velocity(
               &input, bytes, sizeof(expected), &written) == SYNAPSE_CDR_OK);
    assert(written == sizeof(expected));
    assert(memcmp(bytes, expected, sizeof(expected)) == 0);
    assert(synapse_cdr_decode_optical_flow_velocity(
               bytes, sizeof(expected), &output) == SYNAPSE_CDR_OK);
    assert(output.timestamp_ns == input.timestamp_ns);
    assert(output.velocity_flu_m_s[0] == input.velocity_flu_m_s[0]);
    assert(output.velocity_flu_m_s[1] == input.velocity_flu_m_s[1]);
    assert(output.distance_m == input.distance_m);
    assert(output.roll_rad == input.roll_rad);
    assert(output.pitch_rad == input.pitch_rad);
    assert(output.quality == input.quality);
    assert(output.flags == input.flags);
    assert(output.time_status == input.time_status);
    assert(output.id == input.id);
    assert(synapse_cdr_encode_optical_flow_velocity(
               &input, bytes, sizeof(expected) - 1U, NULL) ==
           SYNAPSE_CDR_BUFFER_TOO_SMALL);
    memcpy(bytes, expected, sizeof(expected));
    bytes[sizeof(expected)] = 0U;
    assert(synapse_cdr_decode_optical_flow_velocity(
               bytes, sizeof(bytes), &output) == SYNAPSE_CDR_TRAILING_BYTES);
    bytes[0] = 1U;
    assert(synapse_cdr_decode_optical_flow_velocity(
               bytes, sizeof(expected), &output) ==
           SYNAPSE_CDR_INVALID_ENCAPSULATION);
}

static void test_gnss_fix(void) {
    const synapse_cdr_gnss_fix_t input = {
        .timestamp_ns = UINT64_C(0x0102030405060708),
        .time_unix_ns = UINT64_C(0x1112131415161718),
        .latitude_deg_e7 = INT32_C(425000000),
        .longitude_deg_e7 = -INT32_C(830000000),
        .altitude_msl_mm = INT32_C(123456),
        .altitude_ellipsoid_mm = INT32_C(157890),
        .horizontal_accuracy_mm = 200U,
        .vertical_accuracy_mm = 300U,
        .velocity_accuracy_mm_s = 40U,
        .yaw_accuracy_cdeg = 50U,
        .hdop_centi = 60U,
        .vdop_centi = 70U,
        .ground_speed_cm_s = 800U,
        .course_over_ground_cdeg = 900U,
        .yaw_cdeg = 1000U,
        .velocity_up_cm_s = -110,
        .flags = 0x0fU,
        .fix_type = 3U,
        .satellites_used = 12U,
        .satellites_visible = 20U,
        .time_status = 1U,
        .id = 0U,
    };
    synapse_cdr_gnss_fix_t output = {0};
    uint8_t bytes[SYNAPSE_CDR_GNSS_FIX_TOTAL_BYTES];
    size_t written = 0U;

    assert(synapse_cdr_encode_gnss_fix(
               &input, bytes, sizeof(bytes), &written) == SYNAPSE_CDR_OK);
    assert(written == 64U);
    assert(synapse_cdr_decode_gnss_fix(
               bytes, sizeof(bytes), &output) == SYNAPSE_CDR_OK);
    assert(output.timestamp_ns == input.timestamp_ns);
    assert(output.time_unix_ns == input.time_unix_ns);
    assert(output.latitude_deg_e7 == input.latitude_deg_e7);
    assert(output.longitude_deg_e7 == input.longitude_deg_e7);
    assert(output.altitude_msl_mm == input.altitude_msl_mm);
    assert(output.altitude_ellipsoid_mm == input.altitude_ellipsoid_mm);
    assert(output.horizontal_accuracy_mm == input.horizontal_accuracy_mm);
    assert(output.vertical_accuracy_mm == input.vertical_accuracy_mm);
    assert(output.velocity_accuracy_mm_s == input.velocity_accuracy_mm_s);
    assert(output.yaw_accuracy_cdeg == input.yaw_accuracy_cdeg);
    assert(output.hdop_centi == input.hdop_centi);
    assert(output.vdop_centi == input.vdop_centi);
    assert(output.ground_speed_cm_s == input.ground_speed_cm_s);
    assert(output.course_over_ground_cdeg == input.course_over_ground_cdeg);
    assert(output.yaw_cdeg == input.yaw_cdeg);
    assert(output.velocity_up_cm_s == input.velocity_up_cm_s);
    assert(output.flags == input.flags);
    assert(output.fix_type == input.fix_type);
    assert(output.satellites_used == input.satellites_used);
    assert(output.satellites_visible == input.satellites_visible);
    assert(output.time_status == input.time_status);
    assert(output.id == input.id);
}

int main(void) {
    const synapse_cdr_projection_info_t *gnss =
        synapse_cdr_projection_by_topic_id(8U);
    const synapse_cdr_projection_info_t *optical =
        synapse_cdr_projection_by_topic_id(10U);

    assert(SYNAPSE_CDR_PROJECTION_COUNT == 2U);
    assert(strlen(SYNAPSE_CDR_PROJECTION_SET_IDENTITY) == 64U);
    assert(gnss != NULL && gnss->total_bytes == 64U);
    assert(strcmp(
               gnss->rihs01,
               "RIHS01_ac8d665c1bf6f81796d95bdd6a2285537bbfcb34869ba0e042f8ce24f75d9f0e") ==
           0);
    assert(optical != NULL && optical->total_bytes == 36U);
    assert(strcmp(
               optical->rihs01,
               "RIHS01_8f46bb3da905598105f99e502394842afa66d849de841143565a193074829d09") ==
           0);
    assert(synapse_cdr_projection_by_topic_id(47U) == NULL);
    test_optical_flow_velocity();
    test_gnss_fix();
    return 0;
}
