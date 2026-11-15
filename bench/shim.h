#pragma once

#include <stdint.h>
#include <CoreGraphics/CoreGraphics.h>

#ifdef __cplusplus
extern "C" {
#endif

void kb_init(void);
void kb_load_themes(void);
void kb_stop_audio_unit(void);
void kb_play(int key_code, int type, uint64_t event_time);
void kb_callback(uint32_t type, CGEventRef event);
void kb_render(uint32_t frames, float *left, float *right);
int kb_input_start(void);
int kb_poll_published(uint64_t seq);
void kb_ack(uint64_t seq);

#ifdef __cplusplus
}
#endif
