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

/* Point the asset loader at assets_dir, start SDL and initialise a
 * VideoBeast at time 0. With windowed 0, SDL is forced onto its dummy video
 * driver; otherwise it picks its usual driver (SDL_VIDEODRIVER applies) and
 * the VideoBeast's window shows on screen.
 *
 * Returns NULL, after one stderr line, on every failure it detects: a
 * missing asset file (checked before SDL starts, with the vendored
 * wording), SDL failing to start, SDL offering only a headless driver
 * (offscreen, dummy) for windowed when the environment did not ask for one,
 * or no memory for the VideoBeast. SDL is left shut down, so a later call
 * can succeed. The vendored code still calls exit(1) if SDL starts but the
 * window cannot be created. */
vbshim  *vbshim_create(const char *assets_dir, int windowed);
void     vbshim_destroy(vbshim *h);

/* Advance to ps, which must be the deadline returned by the previous call
 * (0 for the first). Returns the next deadline. */
uint64_t vbshim_tick(vbshim *h, uint64_t ps);

/* addr is the 14-bit offset within the 16 KB window. */
uint8_t  vbshim_read(vbshim *h, uint16_t addr, uint64_t ps);
void     vbshim_write(vbshim *h, uint16_t addr, uint8_t data, uint64_t ps);

/* Number of frame starts the VideoBeast has run (beastem's frameCount). */
uint64_t vbshim_frame_count(vbshim *h);

/* Pixels of the window surface, 0x00RRGGBB per pixel (SDL_PIXELFORMAT_RGB888),
 * or NULL if there is no surface or it has another format. pitch is in bytes.
 * The pointer is valid until the next tick or write. */
const uint32_t *vbshim_frame(vbshim *h, int *width, int *height, int *pitch);

/* Drain SDL's event queue. Returns 1 if it held a quit request or a close
 * of the VideoBeast window, else 0. */
int      vbshim_poll(vbshim *h);

/* 1 if the key at SDL scancode `scancode` was down as of the last
 * vbshim_poll, else 0. */
int      vbshim_key_down(int scancode);

/* Side-effect-free views of VideoBeast state, for dumps. None of them goes
 * through VideoBeast::read, so none changes a register. */

/* The 1 MiB of VRAM, by address. Valid until vbshim_destroy. */
const uint8_t *vbshim_vram(vbshim *h);

/* Register r as stored. 0xFA/0xFB (line counter) and 0xE4-0xE7 (multiply
 * product) hold their values as of the last unlocked CPU read of a register
 * 0x80-0xFD, not live values. */
uint8_t  vbshim_read_register(vbshim *h, uint8_t r);

/* Palette 1 or 2: entry byte >> 1, low byte for even byte, high for odd. */
uint8_t  vbshim_read_palette(vbshim *h, int palette, uint16_t byte);

/* Preload, for --vb-preload. Neither call changes a register as stored. */

/* Copy len bytes to VRAM from addr. The caller checks addr + len <= 1 MiB. */
void     vbshim_preload_vram(vbshim *h, uint32_t addr, const uint8_t *data, uint32_t len);

/* Load palette p (0 = palette 1, 1 = palette 2) from bank (0-3) upward, as a
 * CPU writing the palette window would, 128 bytes per bank, even byte low.
 * Bytes past the end of bank 3 are dropped. */
void     vbshim_preload_palette(vbshim *h, int p, int bank, const uint8_t *data, uint32_t len);

#ifdef __cplusplus
}
#endif
