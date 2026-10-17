// C ABI over the vendored beastem VideoBeast. See vbshim.h.
#include "vbshim.h"

#include <cstdlib>
#include <cstring>
#include <fstream>
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

// The vendored loads end the process with exit(1) on a missing asset file.
// Check the files it will open first, in its order and with its wording, so
// a missing one returns to the caller instead.
static bool assets_present() {
    static const char *const files[] = {"video_registers.mem", "palette_1.mem", "palette_2.mem"};
    static const char *const kinds[] = {"Register", "Palette", "Palette"};
    for (int i = 0; i < 3; i++) {
        const std::string path = assetPath(files[i]);
        std::ifstream file(path.c_str());
        if (!file) {
            std::cout << kinds[i] << " file does not exist: " << path << std::endl;
            return false;
        }
    }
    return true;
}

extern "C" vbshim *vbshim_create(const char *assets_dir, int windowed) {
    // The vendored code logs to std::cout; stdout belongs to the CP/M console.
    std::cout.rdbuf(std::cerr.rdbuf());

    initAssetPath(assets_dir);
    if (!assets_present()) {
        return nullptr;
    }

    // Headless: override priority, so SDL_VIDEODRIVER etc. in the
    // environment cannot win. Windowed: SDL's normal driver choice.
    if (!windowed) {
        SDL_SetHintWithPriority(SDL_HINT_VIDEODRIVER, "dummy", SDL_HINT_OVERRIDE);
    }
    SDL_SetHintWithPriority(SDL_HINT_NO_SIGNAL_HANDLERS, "1", SDL_HINT_OVERRIDE);
    if (SDL_InitSubSystem(SDL_INIT_VIDEO) != 0) {
        // A failed SDL_InitSubSystem undoes itself, so there is nothing to
        // quit here.
        std::cerr << "vbshim: SDL_Init failed: " << SDL_GetError() << std::endl;
        return nullptr;
    }
    // With no display, SDL 2.0.22 and later fall back to the offscreen
    // driver, which would run without a visible window. Unless the
    // environment asked for a driver, treat that as SDL failing to open the
    // window: log it, shut SDL down and fail.
    if (windowed) {
        const char *requested = SDL_getenv("SDL_VIDEODRIVER");
        const char *driver = SDL_GetCurrentVideoDriver();
        const bool headless = driver != nullptr
            && (std::strcmp(driver, "offscreen") == 0 || std::strcmp(driver, "dummy") == 0);
        if (headless && (requested == nullptr || requested[0] == '\0')) {
            std::cerr << "vbshim: --window: no display to open a window on (SDL chose its "
                      << driver << " video driver)" << std::endl;
            SDL_QuitSubSystem(SDL_INIT_VIDEO);
            return nullptr;
        }
    }

    // The constructor leaves VRAM, the registers below 0x80 and the palette
    // entries the asset files do not fill uninitialised. Build the
    // VideoBeast in zeroed storage so they start as 0 on every run.
    void *storage = std::calloc(1, sizeof(VideoBeast));
    if (storage == nullptr) {
        std::cerr << "vbshim: out of memory for the VideoBeast" << std::endl;
        SDL_QuitSubSystem(SDL_INIT_VIDEO);
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

extern "C" int vbshim_poll(vbshim *h) {
    int quit = 0;
    SDL_Event event;
    while (SDL_PollEvent(&event)) {
        if (event.type == SDL_QUIT) {
            quit = 1;
        } else if (event.type == SDL_WINDOWEVENT && event.window.event == SDL_WINDOWEVENT_CLOSE
                   && event.window.windowID == h->window_id) {
            quit = 1;
        }
    }
    return quit;
}

extern "C" int vbshim_key_down(int scancode) {
    int count = 0;
    const Uint8 *state = SDL_GetKeyboardState(&count);
    return scancode >= 0 && scancode < count && state[scancode] != 0;
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

extern "C" void vbshim_preload_vram(vbshim *h, uint32_t addr, const uint8_t *data, uint32_t len) {
    std::memcpy(h->vb->memoryPtr() + addr, data, len);
}

// VideoBeast's REG_LOWER_REG, REG_LOCKED and SET_UNLOCKED, which are private.
static const uint8_t REG_LOWER_REG = 0xF5;
static const uint8_t REG_LOCKED = 0xFE;
static const uint8_t SET_UNLOCKED = 0xF3;

// Goes through VideoBeast::write, the CPU path, so beastem converts each
// entry to a colour for the right palette. VideoBeast::writePalette is not
// used: its palette 2 branch converts the palette 1 entry.
extern "C" void vbshim_preload_palette(vbshim *h, int p, int bank, const uint8_t *data, uint32_t len) {
    const uint8_t saved_select = h->vb->readRegister(REG_LOWER_REG);
    const uint8_t saved_lock = h->vb->readRegister(REG_LOCKED);
    h->vb->writeRegister(REG_LOCKED, SET_UNLOCKED);
    uint32_t done = 0;
    for (int b = bank; b <= 3 && done < len; b++) {
        h->vb->writeRegister(REG_LOWER_REG, static_cast<uint8_t>((p << 2) | b));
        for (uint16_t i = 0; i < 128 && done < len; i++, done++) {
            h->vb->write(0x3F00 + i, data[done], 0);
        }
    }
    h->vb->writeRegister(REG_LOWER_REG, saved_select);
    h->vb->writeRegister(REG_LOCKED, saved_lock);
}
