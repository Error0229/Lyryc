"""QQ Music's QRC cipher: 3DES with a home-grown DES (research only).

Textbook DES (FIPS 46-3) with the two deviations of QQ's implementation, as
documented by WXRIW/QQMusicDecoder: bytes are read and written in little-endian
32-bit words, S-boxes 2 and 4 each carry one altered entry, and the D half of PC-2
is read one bit late (its last pick falls off the end and is always 0). The QRC key is the
well-known constant below; the payload is 3DES-EDE "decrypt" with the key halves
reversed, then zlib.
"""
import zlib

IP = [58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
      64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3,
      61, 53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7]
FP = [IP.index(i + 1) + 1 for i in range(64)]
E = [32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17,
     16, 17, 18, 19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1]
P = [16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10,
     2, 8, 24, 14, 32, 27, 3, 9, 19, 13, 30, 6, 22, 11, 4, 25]
PC1 = [57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3,
       60, 52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6,
       61, 53, 45, 37, 29, 21, 13, 5, 28, 20, 12, 4]
PC2 = [14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2,
       41, 52, 31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32]
PC2_QQ = PC2[:24] + [p + 1 for p in PC2[24:]]
SHIFTS = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1]
S = [
    [14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8,
     4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13],
    [15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5,
     0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9],
    [10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1,
     13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12],
    [7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9,
     10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14],
    [2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6,
     4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3],
    [12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8,
     9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13],
    [4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6,
     1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12],
    [13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2,
     7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11],
]


def perm(x, table, width):
    return sum(((x >> (width - t)) & 1) << (len(table) - 1 - i) for i, t in enumerate(table))


def sbox(rows, quirk):
    """Rows are row-major (row*16+col). The quirk applies QQ's two altered entries."""
    s = [list(r) for r in rows]
    if quirk:
        s[1][23] = 15   # S2 row 1 col 7: 14 -> 15
        s[3][53] = 10   # S4 row 3 col 5: 1 -> 10
    return s


def subkeys(key, quirk):
    k = perm(int.from_bytes(swap(key) if quirk else key, "big"), PC1, 64)
    c, d, out = k >> 28, k & 0xFFFFFFF, []
    for n in SHIFTS:
        c = ((c << n) | (c >> (28 - n))) & 0xFFFFFFF
        d = ((d << n) | (d >> (28 - n))) & 0xFFFFFFF
        out.append(perm(((c << 28) | d) << 1, PC2_QQ, 57) if quirk else perm((c << 28) | d, PC2, 56))
    return out


def feistel(r, k, s):
    x = perm(r, E, 32) ^ k
    y = 0
    for i in range(8):
        b = (x >> (42 - 6 * i)) & 0x3F
        y = (y << 4) | s[i][((b & 0x20) | ((b & 1) << 4)) + ((b >> 1) & 0xF)]
    return perm(y, P, 32)


def swap(b):
    """QQ reads and writes DES blocks as little-endian 32-bit words."""
    return bytes(b[i ^ 3] for i in range(8))


def des(block, keys, s, quirk):
    x = perm(int.from_bytes(swap(block) if quirk else block, "big"), IP, 64)
    l, r = x >> 32, x & 0xFFFFFFFF
    for k in keys:
        l, r = r, l ^ feistel(r, k, s)
    out = perm((r << 32) | l, FP, 64).to_bytes(8, "big")
    return swap(out) if quirk else out


QRC_KEY = b"!@#)(*$%123ZXC!@!@#)(NHL"
_S = sbox(S, True)
_K = [subkeys(QRC_KEY[16:24], True)[::-1], subkeys(QRC_KEY[8:16], True), subkeys(QRC_KEY[0:8], True)[::-1]]


def qrc_decrypt(hex_text):
    data = bytes.fromhex(hex_text)
    out = bytearray()
    for i in range(0, len(data), 8):
        b = data[i:i + 8]
        for keys in _K:
            b = des(b, keys, _S, True)
        out += b
    return zlib.decompress(out).decode("utf-8")


if __name__ == "__main__":
    # FIPS 81 / textbook vector for plain DES.
    k = bytes.fromhex("133457799BBCDFF1")
    assert des(bytes.fromhex("0123456789ABCDEF"), subkeys(k, False), sbox(S, False), False).hex() == "85e813540f0ab405"
    print("textbook DES ok")
