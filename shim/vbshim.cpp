// C ABI over the vendored beastem VideoBeast. See vbshim.h.
#include "vbshim.h"

#include <iostream>

#include "SDL.h"
#include "../vendor/beastem/assets.hpp"
#include "../vendor/beastem/videobeast.hpp"

struct vbshim {
    VideoBeast *vb;
    Uint32 window_id;
};

// Window IDs are not exposed by VideoBeast, so find the window it created:
// the one with the highest ID, since SDL hands IDs out in increasing order.
static Uint32 newest_window_id() {
    Uint32 found = 0;
    for (Uint32 id = 1; id < 4096; id++) {
        if (SDL_GetWindowFromID(id) != nullptr) {
            found = id;
        }
    }
    return found;
}

extern "C" vbshim *vbshim_create(const char *assets_dir) {
    // The vendored code logs to std::cout; stdout belongs to the CP/M console.
    std::cout.rdbuf(std::cerr.rdbuf());

    // Override priority, so SDL_VIDEODRIVER etc. in the environment cannot win.
    SDL_SetHintWithPriority(SDL_HINT_VIDEODRIVER, "dummy", SDL_HINT_OVERRIDE);
    SDL_SetHintWithPriority(SDL_HINT_NO_SIGNAL_HANDLERS, "1", SDL_HINT_OVERRIDE);
    if (SDL_InitSubSystem(SDL_INIT_VIDEO) != 0) {
        std::cerr << "vbshim: SDL_Init failed: " << SDL_GetError() << std::endl;
        return nullptr;
    }

    initAssetPath(assets_dir);

    vbshim *h = new vbshim;
    h->vb = new VideoBeast(1.0f);
    h->vb->init(0, 0);
    h->window_id = newest_window_id();
    return h;
}

extern "C" void vbshim_destroy(vbshim *h) {
    if (h == nullptr) {
        return;
    }
    SDL_Window *window = SDL_GetWindowFromID(h->window_id);
    delete h->vb;
    if (window != nullptr) {
        SDL_DestroyWindow(window);
    }
    delete h;
    SDL_QuitSubSystem(SDL_INIT_VIDEO);
}

extern "C" uint64_t vbshim_tick(vbshim *h, uint64_t ps) {
    return h->vb->tick(ps);
}

extern "C" uint8_t vbshim_read(vbshim *h, uint16_t addr, uint64_t ps) {
    return h->vb->read(addr, ps);
}

extern "C" void vbshim_write(vbshim *h, uint16_t addr, uint8_t data, uint64_t ps) {
    h->vb->write(addr, data, ps);
}

extern "C" const uint32_t *vbshim_frame(vbshim *h, int *width, int *height, int *pitch) {
    SDL_Window *window = SDL_GetWindowFromID(h->window_id);
    if (window == nullptr) {
        return nullptr;
    }
    SDL_Surface *surface = SDL_GetWindowSurface(window);
    if (surface == nullptr || surface->format->BytesPerPixel != 4) {
        return nullptr;
    }
    *width = surface->w;
    *height = surface->h;
    *pitch = surface->pitch;
    return static_cast<const uint32_t *>(surface->pixels);
}
