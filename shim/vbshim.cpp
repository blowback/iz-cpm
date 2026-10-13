// C ABI over the vendored beastem VideoBeast. See vbshim.h.
#include "vbshim.h"

#include <cstdlib>
#include <iostream>
#include <new>

#include "SDL.h"
#include "../vendor/beastem/assets.hpp"
#include "../vendor/beastem/videobeast.hpp"

// VideoBeast::frameCount is private and vendor/beastem/ stays unedited.
// Access checks do not apply to the names in an explicit instantiation, so
// instantiating FrameCountAccess with &VideoBeast::frameCount is allowed, and
// its friend function hands the member pointer out.
template <typename Tag, typename Tag::type Member>
struct FrameCountAccess {
    friend typename Tag::type member_of(Tag) { return Member; }
};
struct FrameCountTag {
    typedef uint64_t VideoBeast::*type;
    friend type member_of(FrameCountTag);
};
template struct FrameCountAccess<FrameCountTag, &VideoBeast::frameCount>;

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

    // The constructor leaves VRAM, the registers below 0x80 and the palette
    // entries the asset files do not fill uninitialised. Build the
    // VideoBeast in zeroed storage so they start as 0 on every run.
    void *storage = std::calloc(1, sizeof(VideoBeast));
    if (storage == nullptr) {
        std::cerr << "vbshim: out of memory for the VideoBeast" << std::endl;
        return nullptr;
    }
    vbshim *h = new vbshim;
    h->vb = new (storage) VideoBeast(1.0f);
    h->vb->init(0, 0);
    h->window_id = newest_window_id();
    return h;
}

extern "C" void vbshim_destroy(vbshim *h) {
    if (h == nullptr) {
        return;
    }
    SDL_Window *window = SDL_GetWindowFromID(h->window_id);
    h->vb->~VideoBeast();
    std::free(h->vb);
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

extern "C" uint64_t vbshim_frame_count(vbshim *h) {
    return h->vb->*member_of(FrameCountTag());
}

extern "C" const uint32_t *vbshim_frame(vbshim *h, int *width, int *height, int *pitch) {
    SDL_Window *window = SDL_GetWindowFromID(h->window_id);
    if (window == nullptr) {
        return nullptr;
    }
    SDL_Surface *surface = SDL_GetWindowSurface(window);
    if (surface == nullptr || surface->format->format != SDL_PIXELFORMAT_RGB888) {
        return nullptr;
    }
    *width = surface->w;
    *height = surface->h;
    *pitch = surface->pitch;
    return static_cast<const uint32_t *>(surface->pixels);
}

extern "C" const uint8_t *vbshim_vram(vbshim *h) {
    return h->vb->memoryPtr();
}

extern "C" uint8_t vbshim_read_register(vbshim *h, uint8_t r) {
    return h->vb->readRegister(r);
}

extern "C" uint8_t vbshim_read_palette(vbshim *h, int palette, uint16_t byte) {
    return h->vb->readPalette(palette, byte);
}
