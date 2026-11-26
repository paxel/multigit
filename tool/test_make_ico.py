import struct
import unittest

from make_ico import PNG_SIGNATURE, ico_bytes


def png(width: int, height: int) -> bytes:
    return PNG_SIGNATURE + b"\0\0\0\rIHDR" + struct.pack(">II", width, height) + b"rest"


class IcoBytesTest(unittest.TestCase):
    def test_the_png_follows_one_directory_entry(self):
        image = png(256, 256)
        icon = ico_bytes(image)
        self.assertEqual(struct.unpack("<HHH", icon[:6]), (0, 1, 1))
        width, height, _, _, planes, bits, size, offset = struct.unpack("<BBBBHHII", icon[6:22])
        self.assertEqual((width, height, planes, bits), (0, 0, 1, 32))
        self.assertEqual((size, offset), (len(image), 22))
        self.assertEqual(icon[22:], image)

    def test_only_a_256_pixel_png_is_taken(self):
        with self.assertRaises(ValueError):
            ico_bytes(png(128, 128))
        with self.assertRaises(ValueError):
            ico_bytes(b"GIF89a" + b"\0" * 30)


if __name__ == "__main__":
    unittest.main()
