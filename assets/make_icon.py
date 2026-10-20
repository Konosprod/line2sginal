"""Dessine l'icône de l'application : python3 assets/make_icon.py assets"""
import sys
from PIL import Image, ImageDraw, ImageFilter
out = sys.argv[1]
N = 4096
def lerp(a, b, t): return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))

# Fond : carré arrondi, dégradé diagonal vert -> bleu.
m = int(N * 0.06)
grad = Image.new("RGB", (N, N))
small = Image.new("RGB", (256, 256))
px = small.load()
for y in range(256):
    for x in range(256):
        px[x, y] = lerp((43, 182, 115), (59, 111, 224), (x + y) / 510)
grad = small.resize((N, N), Image.BICUBIC)
mask = Image.new("L", (N, N), 0)
ImageDraw.Draw(mask).rounded_rectangle((m, m, N - m, N - m), radius=int(N * 0.2), fill=255)
icon = Image.new("RGBA", (N, N), (0, 0, 0, 0))
icon.paste(grad, (0, 0), mask)

# Sticker blanc au coin décollé, avec un visage.
layer = Image.new("RGBA", (N, N), (0, 0, 0, 0))
d = ImageDraw.Draw(layer)
s = int(N * 0.54); L = (N - s) // 2; T = (N - s) // 2; R = L + s; B = T + s; c = int(s * 0.28)
d.rounded_rectangle((L, T, R, B), radius=int(s * 0.16), fill=(255, 255, 255, 255))
d.polygon([(R - c, B), (R, B - c), (R + 50, B + 50)], fill=(0, 0, 0, 0))
d.rectangle((R - c // 2, B - c // 2, R + 50, B + 50), fill=(0, 0, 0, 0))
d.polygon([(R - c, B), (R, B - c), (R - c, B - c)], fill=(206, 220, 236, 255))
ink = (31, 59, 87, 255)
ex = int(s * 0.07); ey = int(s * 0.09); cy = T + int(s * 0.40)
for cx in (L + int(s * 0.33), L + int(s * 0.62)):
    d.ellipse((cx - ex, cy - ey, cx + ex, cy + ey), fill=ink)
w = int(s * 0.055)
d.arc((L + int(s * 0.25), T + int(s * 0.36), L + int(s * 0.70), T + int(s * 0.74)), start=25, end=155, fill=ink, width=w)
for cx in (L + int(s * 0.19), L + int(s * 0.76)):
    d.ellipse((cx - ex, cy + int(s*0.13) - ex // 2, cx + ex, cy + int(s*0.13) + ex // 2), fill=(255, 150, 160, 200))
layer = layer.rotate(8, resample=Image.BICUBIC, center=(N / 2, N / 2))

shadow = Image.new("RGBA", (N, N), (0, 0, 0, 0))
shadow.putalpha(layer.getchannel("A").point(lambda a: a * 0.28))
shadow = shadow.filter(ImageFilter.GaussianBlur(N * 0.02))
icon.alpha_composite(shadow, (0, int(N * 0.02)))
icon.alpha_composite(layer)

big = icon.resize((1024, 1024), Image.LANCZOS)
big.save(out + "/icon.png", optimize=True)
big.save(out + "/icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
big.save(out + "/icon.icns")
big.resize((256, 256), Image.LANCZOS).save(out + "/preview256.png")
big.resize((32, 32), Image.LANCZOS).resize((128, 128), Image.NEAREST).save(out + "/preview32.png")
