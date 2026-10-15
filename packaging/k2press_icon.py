"""K2 Press Desktop app icon: three post cards fanned on a navy tile (original artwork)."""
import sys, math
from PIL import Image, ImageDraw, ImageFont, ImageFilter

S = 1024
NAVY_TOP, NAVY_BOT = (16, 30, 56), (8, 12, 26)
TEAL, GREEN, CARD, INK = (0, 180, 200), (0, 200, 150), (246, 249, 251), (10, 15, 30)

def tile(size, ss=4):
    W = size * ss
    im = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    # vertical gradient
    g = Image.new("RGBA", (W, W))
    gd = ImageDraw.Draw(g)
    for y in range(W):
        t = y / (W - 1)
        c = tuple(int(NAVY_TOP[i] * (1 - t) + NAVY_BOT[i] * t) for i in range(3))
        gd.line([(0, y), (W, y)], fill=c + (255,))
    # teal glow, top right
    glow = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([W * 0.35, -W * 0.25, W * 1.25, W * 0.6], fill=TEAL + (70,))
    glow = glow.filter(ImageFilter.GaussianBlur(W * 0.12))
    g = Image.alpha_composite(g, glow)
    mask = Image.new("L", (W, W), 0)
    r = int(W * 0.225)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, W - 1, W - 1], radius=r, fill=255)
    im.paste(g, (0, 0), mask)
    return im, W

def card(W, w, h, fill, angle, cx, cy, radius, front=False, small=False):
    layer = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    c = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(c)
    d.rounded_rectangle([0, 0, w - 1, h - 1], radius=radius, fill=fill + (255,))
    if front:
        pad = int(w * 0.13)
        if small:
            # Small sizes: just the mark.
            font = ImageFont.truetype("C:/Windows/Fonts/arialbd.ttf", int(h * 0.42))
            d.text((w / 2, h * 0.47), "K2", font=font, fill=INK + (255,), anchor="mm")
            d.rounded_rectangle([pad, int(h * 0.76), w - pad, int(h * 0.76) + max(2, int(h * 0.07))], radius=int(h * 0.035), fill=TEAL + (255,))
        else:
            font = ImageFont.truetype("C:/Windows/Fonts/arialbd.ttf", int(h * 0.33))
            d.text((pad - int(w * 0.02), int(h * 0.09)), "K2", font=font, fill=INK + (255,), anchor="la")
            y = int(h * 0.56)
            d.rounded_rectangle([pad, y, pad + int(w * 0.30), y + int(h * 0.045)], radius=int(h * 0.02), fill=TEAL + (255,))
            for k, frac in enumerate([0.74, 0.58]):
                yy = y + int(h * 0.11) + k * int(h * 0.1)
                d.rounded_rectangle([pad, yy, pad + int((w - 2 * pad) * frac), yy + int(h * 0.04)], radius=int(h * 0.02), fill=(180, 190, 205, 255))
    c = c.rotate(angle, resample=Image.BICUBIC, expand=True)
    # soft shadow
    sh = Image.new("RGBA", c.size, (0, 0, 0, 0))
    sh.putalpha(c.split()[3].point(lambda a: int(a * 0.45)))
    sh = sh.filter(ImageFilter.GaussianBlur(max(1, w * 0.04)))
    x, y = int(cx - c.width / 2), int(cy - c.height / 2)
    layer.alpha_composite(sh, (x + int(w * 0.02), y + int(w * 0.05)))
    layer.alpha_composite(c, (x, y))
    return layer

def icon(size):
    small = size <= 48
    im, W = tile(size)
    w, h = int(W * 0.44), int(W * 0.55)
    rad = int(w * 0.11)
    cx, cy = W * 0.5, W * 0.53
    if not small:
        im = Image.alpha_composite(im, card(W, w, h, GREEN, 16, cx - W * 0.12, cy - W * 0.02, rad))
        im = Image.alpha_composite(im, card(W, w, h, TEAL, 7, cx - W * 0.05, cy - W * 0.01, rad))
        im = Image.alpha_composite(im, card(W, w, h, CARD, -4, cx + W * 0.06, cy + W * 0.01, rad, front=True))
    else:
        w, h = int(W * 0.56), int(W * 0.70)
        im = Image.alpha_composite(im, card(W, w, h, TEAL, 10, cx - W * 0.06, cy - W * 0.01, int(w * 0.12)))
        im = Image.alpha_composite(im, card(W, w, h, CARD, -3, cx + W * 0.05, cy + W * 0.01, int(w * 0.12), front=True, small=True))
    return im.resize((size, size), Image.LANCZOS)

out = sys.argv[1]
icon(1024).save(f"{out}/k2press-1024.png")
for s in [16, 24, 32, 48, 64, 128, 256, 512]:
    icon(s).save(f"{out}/k2press-{s}.png")
imgs = [icon(s) for s in [256, 128, 64, 48, 32, 24, 16]]
imgs[0].save(f"{out}/k2press.ico", sizes=[(s, s) for s in [256, 128, 64, 48, 32, 24, 16]], append_images=imgs[1:])
icon(1024).save(f"{out}/k2press.icns")
print("ok")
