from functools import reduce

from sapio_compiler import *
from sapio_zoo.multisig import *
import os
import unittest
from .testutil import random_k


class TestMultiSig(unittest.TestCase):
    def test_multisig(self):
        a = RawMultiSig(keys=[random_k() for _ in range(5)], thresh=2)
        b = RawMultiSigWithPath(
            keys=[random_k() for _ in range(5)],
            thresh_all=3,
            thresh_path=2,
            amount=Bitcoin(5),
            path=a,
        )


if __name__ == "__main__":
    unittest.main()
