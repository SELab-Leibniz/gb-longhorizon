/*
 * sameboy_runner — headless reference run for golden-hash generation.
 *
 *   sameboy_runner ROM FRAMES INPUT_SCRIPT OUT_DIR [--boot dmg_boot.bin]
 *                  [--dump-every N] [--boot-delay M]
 *
 * Runs ROM on a SameBoy DMG-B core for FRAMES frames after the boot ROM has
 * handed over to the cartridge, applying INPUT_SCRIPT (same format as
 * gb-cli: "<frame> <BUTTON,BUTTON>" per line) and writes:
 *
 *   OUT_DIR/hashes.txt            one line per frame: "<frame> <fnv1a64>"
 *                                 over the 160×144 2-bit shade buffer, exactly
 *                                 as gb_core::util::fnv1a64(framebuffer)
 *   OUT_DIR/frame_XXXXXX.pgm      every --dump-every frames (default 60)
 *
 * Boot ROM: SameBoy needs one. Pass SameBoy's own free dmg_boot.bin with
 * --boot for maximum fidelity (requires rgbds to build). Without it, an
 * embedded 64-byte stub sets the documented post-boot register and I/O
 * state and hands over to 0x0100 within the first frame. Either way frame
 * numbering starts when the boot ROM is unmapped (FF50 write), which is
 * the agent emulator's frame 0 (DECISIONS.md D3).
 *
 * Frame alignment between two emulators is only exact to ±1–2 frames
 * (SameBoy counts vblanks; gb-core counts 70 224-cycle blocks from PC=0100),
 * which is why grade.py matches each agent frame against the golden frames
 * within a small window and why input scripts should hold buttons ≥ 10
 * frames.
 *
 * Build: see build.sh (clones SameBoy at a pinned commit, builds
 * libsameboy.a with GB_INTERNAL visible, links this file).
 */

#ifndef GB_INTERNAL
#define GB_INTERNAL
#endif
#include <Core/gb.h>

#include <ctype.h>
#include <errno.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#define W 160
#define H 144

static uint32_t pixels[W * H];      /* SameBoy renders into this progressively */
static uint32_t last_frame[W * H];  /* copied at vblank: the last COMPLETED frame */
static uint8_t frame_bytes[W * H * 2];
static size_t frame_len = W * H;    /* bytes hashed per frame: W*H (DMG shades) or 2*W*H (CGB RGB555 LE) */
static bool frame_ready = false;
static bool is_cgb = false;
static bool hit_breakpoint = false;

/* ---- embedded boot stub ----------------------------------------------- */
/* Sets AF=01B0 BC=0013 DE=00D8 HL=014D SP=FFFE, LCDC=91 BGP=FC OBP0/1=FF,
 * NR52=80 NR50=77 NR51=F3 NR11=80 NR12=F3, then LDH (FF50),A at 0x00FE. */
static uint8_t boot_stub[256];

static void init_boot_stub(unsigned delay, bool cgb)
{
    /* DMG: documented DMG post-boot state. */
    static const uint8_t prog_dmg[] = {
        0x3E, 0x91, 0xE0, 0x40, /* LD A,91 ; LDH (40),A  LCDC */
        0x3E, 0xFC, 0xE0, 0x47, /* BGP */
        0x3E, 0xFF, 0xE0, 0x48, /* OBP0 */
        0xE0, 0x49,             /* OBP1 */
        0x3E, 0x80, 0xE0, 0x26, /* NR52 */
        0x3E, 0x77, 0xE0, 0x24, /* NR50 */
        0x3E, 0xF3, 0xE0, 0x25, /* NR51 */
        0x3E, 0x80, 0xE0, 0x11, /* NR11 */
        0x3E, 0xF3, 0xE0, 0x12, /* NR12 */
        0x3E, 0xB0, 0xE0, 0x80, /* (FF80) = B0  -> F */
        0x3E, 0x01, 0xE0, 0x81, /* (FF81) = 01  -> A */
        0x01, 0x13, 0x00,       /* LD BC,0013 */
        0x11, 0xD8, 0x00,       /* LD DE,00D8 */
        0x21, 0x4D, 0x01,       /* LD HL,014D */
        0x31, 0x80, 0xFF,       /* LD SP,FF80 */
        0xF1,                   /* POP AF -> AF=01B0, SP=FF82 */
        0x31, 0xFE, 0xFF,       /* LD SP,FFFE */
        0xC3, 0x00, 0x00,       /* JP 00FE-delay (patched below) */
    };
    /* CGB: select CGB mode through KEY0 (only writable while the boot ROM is
     * mapped), then the documented CGB post-boot state:
     * AF=1180 BC=0000 DE=FF56 HL=000D SP=FFFE, LCDC=91. */
    static const uint8_t prog_cgb[] = {
        0x3E, 0x80, 0xE0, 0x4C, /* LD A,80 ; LDH (4C),A  KEY0 = CGB mode */
        0x3E, 0x91, 0xE0, 0x40, /* LCDC */
        0x3E, 0x80, 0xE0, 0x26, /* NR52 */
        0x3E, 0x77, 0xE0, 0x24, /* NR50 */
        0x3E, 0xF3, 0xE0, 0x25, /* NR51 */
        0x3E, 0x80, 0xE0, 0x80, /* (FF80) = 80  -> F */
        0x3E, 0x11, 0xE0, 0x81, /* (FF81) = 11  -> A */
        0x01, 0x00, 0x00,       /* LD BC,0000 */
        0x11, 0x56, 0xFF,       /* LD DE,FF56 */
        0x21, 0x0D, 0x00,       /* LD HL,000D */
        0x31, 0x80, 0xFF,       /* LD SP,FF80 */
        0xF1,                   /* POP AF -> AF=1180 */
        0x31, 0xFE, 0xFF,       /* LD SP,FFFE */
        0xC3, 0x00, 0x00,       /* JP 00FE-delay (patched below) */
    };
    const uint8_t *prog = cgb ? prog_cgb : prog_dmg;
    size_t len = cgb ? sizeof prog_cgb : sizeof prog_dmg;
    memset(boot_stub, 0x00, sizeof boot_stub); /* 0x00 = NOP */
    memcpy(boot_stub, prog, len);
    /* --boot-delay N runs N extra NOPs (N M-cycles) before handing over, which
     * shifts the DIV and PPU phase at PC=0100. The golden builder uses this to
     * find frames that do not depend on sub-frame timing. */
    unsigned max_delay = 0xFE - (unsigned)len;
    if (delay > max_delay) { fprintf(stderr, "--boot-delay max is %u\n", max_delay); exit(1); }
    unsigned target = 0xFE - delay;
    boot_stub[len - 2] = (uint8_t)(target & 0xFF);
    boot_stub[len - 1] = (uint8_t)(target >> 8);
    boot_stub[0xFE] = 0xE0; /* LDH (FF50),A — A is non-zero (01 / 11) here */
    boot_stub[0xFF] = 0x50;
}

/* ---- SameBoy callbacks ------------------------------------------------ */

static uint32_t rgb_encode(GB_gameboy_t *gb, uint8_t r, uint8_t g, uint8_t b)
{
    (void)gb;
    if (is_cgb) {
        /* Colour correction is disabled, so SameBoy hands us (c5 << 3) | (c5 >> 2). */
        return (uint32_t)(r >> 3) | ((uint32_t)(g >> 3) << 5) | ((uint32_t)(b >> 3) << 10);
    }
    (void)g; (void)b;
    /* GREY palette: FF,AA,55,00 → shade 0..3 (0 = lightest). */
    return 3u - (uint32_t)r * 4u / 256u;
}

static void on_vblank(GB_gameboy_t *gb, GB_vblank_type_t type)
{
    (void)gb; (void)type;
    memcpy(last_frame, pixels, sizeof pixels);
    frame_ready = true;
}

static void on_execute(GB_gameboy_t *gb, uint16_t address, uint8_t opcode)
{
    (void)address;
    if (opcode == 0x40 && gb->boot_rom_finished) hit_breakpoint = true;  /* LD B,B */
}

static void on_log(GB_gameboy_t *gb, const char *string, GB_log_attributes_t attributes)
{
    (void)gb; (void)string; (void)attributes; /* silence debugger chatter */
}

/* ---- helpers ---------------------------------------------------------- */

static uint64_t fnv1a64(const uint8_t *p, size_t n)
{
    uint64_t h = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < n; i++) {
        h ^= p[i];
        h *= 0x100000001b3ULL;
    }
    return h;
}

static void capture_frame(void)
{
    if (is_cgb) {
        for (size_t i = 0; i < W * H; i++) {
            frame_bytes[2 * i] = (uint8_t)(last_frame[i] & 0xFF);
            frame_bytes[2 * i + 1] = (uint8_t)(last_frame[i] >> 8);
        }
        frame_len = 2 * W * H;
    } else {
        for (size_t i = 0; i < W * H; i++) frame_bytes[i] = (uint8_t)(last_frame[i] & 3);
        frame_len = W * H;
    }
}

/* DMG: binary PGM (P5), 0 = black … 255 = white. CGB: binary PPM (P6), 8-bit
 * channels expanded as (c5 << 3) | (c5 >> 2). Same formats gb-cli writes. */
static void write_image(const char *path)
{
    FILE *f = fopen(path, "wb");
    if (!f) { perror(path); exit(1); }
    if (is_cgb) {
        fprintf(f, "P6\n%d %d\n255\n", W, H);
        for (size_t i = 0; i < W * H; i++) {
            uint32_t c = last_frame[i];
            for (int k = 0; k < 3; k++) {
                uint8_t v = (uint8_t)((c >> (5 * k)) & 31);
                fputc((v << 3) | (v >> 2), f);
            }
        }
    } else {
        fprintf(f, "P5\n%d %d\n255\n", W, H);
        static const uint8_t grey[4] = {255, 170, 85, 0};
        for (size_t i = 0; i < W * H; i++) fputc(grey[last_frame[i] & 3], f);
    }
    fclose(f);
}

typedef struct { uint64_t frame; uint8_t mask; } script_line_t;

static int parse_buttons(const char *s, uint8_t *mask)
{
    *mask = 0;
    char buf[256];
    strncpy(buf, s, sizeof buf - 1);
    buf[sizeof buf - 1] = 0;
    for (char *tok = strtok(buf, ", \t\r\n"); tok; tok = strtok(NULL, ", \t\r\n")) {
        for (char *c = tok; *c; c++) *c = (char)toupper((unsigned char)*c);
        if      (!strcmp(tok, "RIGHT"))  *mask |= 1 << GB_KEY_RIGHT;
        else if (!strcmp(tok, "LEFT"))   *mask |= 1 << GB_KEY_LEFT;
        else if (!strcmp(tok, "UP"))     *mask |= 1 << GB_KEY_UP;
        else if (!strcmp(tok, "DOWN"))   *mask |= 1 << GB_KEY_DOWN;
        else if (!strcmp(tok, "A"))      *mask |= 1 << GB_KEY_A;
        else if (!strcmp(tok, "B"))      *mask |= 1 << GB_KEY_B;
        else if (!strcmp(tok, "SELECT")) *mask |= 1 << GB_KEY_SELECT;
        else if (!strcmp(tok, "START"))  *mask |= 1 << GB_KEY_START;
        else return -1;
    }
    return 0;
}

static size_t load_script(const char *path, script_line_t **out)
{
    *out = NULL;
    if (!strcmp(path, "/dev/null") || !strcmp(path, "-")) return 0;
    FILE *f = fopen(path, "r");
    if (!f) { perror(path); exit(1); }
    size_t n = 0, cap = 0;
    char line[512];
    unsigned lineno = 0;
    uint64_t last = 0; bool have_last = false;
    while (fgets(line, sizeof line, f)) {
        lineno++;
        char *hash = strchr(line, '#');
        if (hash) *hash = 0;
        char *p = line;
        while (*p == ' ' || *p == '\t') p++;
        if (*p == 0 || *p == '\n' || *p == '\r') continue;
        char *end;
        uint64_t frame = strtoull(p, &end, 10);
        if (end == p) { fprintf(stderr, "%s:%u: bad frame number\n", path, lineno); exit(1); }
        if (have_last && frame <= last) { fprintf(stderr, "%s:%u: frames must ascend\n", path, lineno); exit(1); }
        last = frame; have_last = true;
        uint8_t mask;
        if (parse_buttons(end, &mask)) { fprintf(stderr, "%s:%u: unknown button\n", path, lineno); exit(1); }
        if (n == cap) { cap = cap ? cap * 2 : 16; *out = realloc(*out, cap * sizeof **out); }
        (*out)[n].frame = frame; (*out)[n].mask = mask; n++;
    }
    fclose(f);
    return n;
}

static void apply_mask(GB_gameboy_t *gb, uint8_t mask)
{
    for (int k = 0; k < GB_KEY_MAX; k++) GB_set_key_state(gb, (GB_key_t)k, (mask >> k) & 1);
}

static void run_one_frame(GB_gameboy_t *gb)
{
    frame_ready = false;
    while (!frame_ready && !hit_breakpoint) GB_run_frame(gb);
    capture_frame();
}

/* ---- main ------------------------------------------------------------- */

int main(int argc, char **argv)
{
    if (argc < 5) {
        fprintf(stderr, "usage: %s ROM FRAMES INPUT_SCRIPT OUT_DIR [--boot dmg_boot.bin] [--dump-every N] [--boot-delay M]\n", argv[0]);
        return 1;
    }
    const char *rom_path = argv[1];
    uint64_t frames = strtoull(argv[2], NULL, 10);
    const char *script_path = argv[3];
    const char *out_dir = argv[4];
    const char *boot_path = NULL;
    uint64_t dump_every = 60;
    unsigned boot_delay = 0;
    bool until_breakpoint = false;
    for (int i = 5; i < argc; i++) {
        if (!strcmp(argv[i], "--boot") && i + 1 < argc) boot_path = argv[++i];
        else if (!strcmp(argv[i], "--dump-every") && i + 1 < argc) dump_every = strtoull(argv[++i], NULL, 10);
        else if (!strcmp(argv[i], "--boot-delay") && i + 1 < argc) boot_delay = (unsigned)strtoul(argv[++i], NULL, 10);
        else if (!strcmp(argv[i], "--model") && i + 1 < argc) {
            const char *m = argv[++i];
            if (!strcmp(m, "cgb")) is_cgb = true;
            else if (strcmp(m, "dmg")) { fprintf(stderr, "--model must be dmg or cgb\n"); return 1; }
        }
        else if (!strcmp(argv[i], "--until-breakpoint")) until_breakpoint = true;
        else { fprintf(stderr, "unknown arg %s\n", argv[i]); return 1; }
    }

    if (mkdir(out_dir, 0755) && errno != EEXIST) { perror(out_dir); return 1; }

    script_line_t *script;
    size_t script_n = load_script(script_path, &script);

    /* SameBoy randomises power-on RAM with a time-seeded RNG, which makes runs
     * non-reproducible. Disable it: RAM starts zeroed, matching gb-core's scaffold
     * (vec![0; ..]) and making goldens identical across runs and platforms. */
    GB_random_set_enabled(false);
    GB_gameboy_t gb;
    /* DMG-B for DMG, CGB-C for CGB (the revision the Mealybug CGB expectations target). */
    GB_init(&gb, is_cgb ? GB_MODEL_CGB_C : GB_MODEL_DMG_B);
    if (boot_path) {
        if (GB_load_boot_rom(&gb, boot_path)) { fprintf(stderr, "cannot load boot ROM %s\n", boot_path); return 1; }
    } else {
        init_boot_stub(boot_delay, is_cgb);
        GB_load_boot_rom_from_buffer(&gb, boot_stub, sizeof boot_stub);
    }
    GB_set_palette(&gb, &GB_PALETTE_GREY);
    GB_set_rgb_encode_callback(&gb, rgb_encode);
    GB_set_pixels_output(&gb, pixels);
    GB_set_vblank_callback(&gb, on_vblank);
    GB_set_execution_callback(&gb, on_execute);
    GB_set_log_callback(&gb, on_log);
    GB_set_color_correction_mode(&gb, GB_COLOR_CORRECTION_DISABLED);
    GB_set_emulate_joypad_bouncing(&gb, false);
    GB_set_rtc_mode(&gb, GB_RTC_MODE_ACCURATE);
    GB_set_turbo_mode(&gb, true, true);
    if (GB_load_rom(&gb, rom_path)) { perror(rom_path); return 1; }

    /* Run the boot ROM out. With the stub this is < 1 frame; with the real
     * boot ROM it is ~1 s of logo animation. Frame 0 is the frame in which
     * FF50 was written, matching gb-core's "start at PC=0100" convention. */
    unsigned boot_frames = 0;
    while (!gb.boot_rom_finished) {
        run_one_frame(&gb);
        if (++boot_frames > 600) { fprintf(stderr, "boot ROM never finished\n"); return 1; }
    }

    char path[4096];
    snprintf(path, sizeof path, "%s/hashes.txt", out_dir);
    FILE *hashes = fopen(path, "w");
    if (!hashes) { perror(path); return 1; }

    size_t si = 0;
    for (uint64_t frame = 0; frame < frames; frame++) {
        while (si < script_n && script[si].frame == frame) apply_mask(&gb, script[si++].mask);
        run_one_frame(&gb);
        uint64_t n = frame + 1; /* 1-based, like gb-cli's --dump-every numbering */
        fprintf(hashes, "%llu %016llx\n", (unsigned long long)n, (unsigned long long)fnv1a64(frame_bytes, frame_len));
        if (dump_every && n % dump_every == 0) {
            snprintf(path, sizeof path, "%s/frame_%06llu.%s", out_dir, (unsigned long long)n, is_cgb ? "ppm" : "pgm");
            write_image(path);
        }
        if (until_breakpoint && hit_breakpoint) {
            /* The screenshot a test ROM means at LD B,B is the last completed frame. */
            snprintf(path, sizeof path, "%s/breakpoint.%s", out_dir, is_cgb ? "ppm" : "pgm");
            write_image(path);
            uint16_t bc = gb.registers[GB_REGISTER_BC], de = gb.registers[GB_REGISTER_DE], hl = gb.registers[GB_REGISTER_HL];
            printf("breakpoint %llu %016llx regs %u %u %u %u %u %u\n", (unsigned long long)n,
                   (unsigned long long)fnv1a64(frame_bytes, frame_len),
                   bc >> 8, bc & 0xFF, de >> 8, de & 0xFF, hl >> 8, hl & 0xFF);
            fclose(hashes);
            free(script);
            return 0;
        }
    }
    if (until_breakpoint) {
        printf("no-breakpoint\n");
        fclose(hashes);
        free(script);
        return 3;
    }
    fclose(hashes);
    fprintf(stderr, "ok: %llu frames (%u boot frames skipped) -> %s\n",
            (unsigned long long)frames, boot_frames, out_dir);
    free(script);
    return 0;
}
