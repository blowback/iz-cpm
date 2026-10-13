/* C ABI over the vendored beastem VideoBeast (vendor/beastem/).
 *
 * This is the only code that talks to the vendored C++. Time is in
 * picoseconds of emulated time. One handle per process: SDL video is
 * process-global and not thread-safe.
 */
#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct vbshim vbshim;

/* Start SDL with the dummy video driver, point the asset loader at
 * assets_dir and initialise a VideoBeast at time 0. The vendored code calls
 * exit(1) if an asset is missing or the window cannot be created. */
vbshim  *vbshim_create(const char *assets_dir);
void     vbshim_destroy(vbshim *h);

/* Advance to ps, which must be the deadline returned by the previous call
 * (0 for the first). Returns the next deadline. */
uint64_t vbshim_tick(vbshim *h, uint64_t ps);

/* addr is the 14-bit offset within the 16 KB window. */
uint8_t  vbshim_read(vbshim *h, uint16_t addr, uint64_t ps);
void     vbshim_write(vbshim *h, uint16_t addr, uint8_t data, uint64_t ps);

/* Number of frame starts the VideoBeast has run (beastem's frameCount). */
uint64_t vbshim_frame_count(vbshim *h);

/* Pixels of the window surface (32 bits per pixel), or NULL. pitch is in
 * bytes. The pointer is valid until the next tick or write. */
const uint32_t *vbshim_frame(vbshim *h, int *width, int *height, int *pitch);

#ifdef __cplusplus
}
#endif
