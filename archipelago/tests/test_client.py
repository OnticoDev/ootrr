import os
import sys
import time
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import tempfile

from ootrr.client import NUDGE, PENDING, RETRY, Deliverer, got, recv
from ootrr import client


class DelivererAsksAgainAfterADroppedGive(unittest.TestCase):
    def test_a_give_dropped_while_paused_is_asked_for_again(self):
        d = Deliverer()
        state = {got("a"): False, got("b"): False, PENDING: False}
        self.assertEqual(d.next(["a", "b"], state, 0.0), "a")
        state[recv("a")] = True
        state[PENDING] = True
        self.assertIsNone(d.next(["a", "b"], state, 0.5))
        state[PENDING] = False
        self.assertIsNone(d.next(["a", "b"], state, 1.0))
        self.assertEqual(d.next(["a", "b"], state, NUDGE), "a")
        self.assertIsNone(d.next(["a", "b"], state, NUDGE + 1.0))
        for t in range(1, 60):
            self.assertIn(d.next(["a", "b"], state, NUDGE * (t + 1)), ("a", None))
        self.assertNotIn("a", d.aside)
        state[recv("a")] = False
        state[got("a")] = True
        self.assertEqual(d.next(["a", "b"], state, 500.0), "b")

    def test_a_give_that_ran_is_not_asked_for_again_before_its_retry(self):
        d = Deliverer()
        state = {got("a"): False, PENDING: False}
        self.assertEqual(d.next(["a"], state, 0.0), "a")
        self.assertIsNone(d.next(["a"], state, RETRY - 1.0))
        self.assertEqual(d.next(["a"], state, RETRY), "a")


HERE = os.path.dirname(os.path.abspath(__file__))


def script(d, name, *lines):
    path = os.path.join(d, name)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(["import os, sys, time", "open(os.environ['OOTRR_TEST_STARTS'], 'a').write('x')", *lines, ""]))
    return path


def starts(d):
    path = os.path.join(d, "starts")
    if not os.path.exists(path):
        return 0
    with open(path, encoding="utf-8") as f:
        return len(f.read())


class BridgeRefusesAnExeThatIsNotOOTRR(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.d = self.dir.name
        os.environ["OOTRR_TEST_STARTS"] = os.path.join(self.d, "starts")
        self.wait = client.HELLO_WAIT

    def tearDown(self):
        client.HELLO_WAIT = self.wait
        os.environ.pop("OOTRR_TEST_STARTS", None)
        self.dir.cleanup()

    def test_an_exe_that_quits_at_once_is_started_once(self):
        b = client.Bridge(script(self.d, "quits.py", "sys.exit(0)"))
        for _ in range(5):
            r = b.call({"op": "read", "bools": [], "keys": []})
            self.assertFalse(r["ok"])
            self.assertIn("did not answer as the OOTRR app", r["error"])
        self.assertEqual(starts(self.d), 1)
        self.assertIsNone(b.p)

    def test_an_exe_that_never_answers_is_stopped_and_not_started_again(self):
        client.HELLO_WAIT = 1.0
        b = client.Bridge(script(self.d, "hangs.py", "time.sleep(60)"))
        t = time.monotonic()
        r = b.call({"op": "read", "bools": [], "keys": []})
        self.assertLess(time.monotonic() - t, 10.0)
        self.assertFalse(r["ok"])
        self.assertIn("did not answer as the OOTRR app", r["error"])
        self.assertFalse(b.call({"op": "read"})["ok"])
        self.assertEqual(starts(self.d), 1)
        self.assertIsNone(b.p)

    def test_an_exe_that_answers_something_else_is_refused(self):
        b = client.Bridge(script(self.d, "talks.py", "sys.stdin.readline()", "print('[1, 2]', flush=True)", "time.sleep(60)"))
        self.assertFalse(b.call({"op": "read"})["ok"])
        self.assertIsNotNone(b.bad)
        self.assertIsNone(b.p)

    def test_a_missing_exe_is_reported(self):
        b = client.Bridge(os.path.join(self.d, "missing", "ootrr.exe"))
        r = b.call({"op": "read"})
        self.assertFalse(r["ok"])
        self.assertIn("Could not run", r["error"])
        self.assertIn("ootrr_exe", r["error"])

    def test_the_ootrr_bridge_is_accepted(self):
        os.environ.update(OOTRR_FAKE_STATE=os.path.join(self.d, "state.json"), OOTRR_FAKE_SEED_KEY="k")
        try:
            b = client.Bridge(os.path.join(HERE, "fake_bridge.py"))
            r = b.call({"op": "read", "bools": ["OOTRR.Seed.k"], "keys": []})
            self.assertTrue(r["ok"], r)
            self.assertTrue(r["bools"]["OOTRR.Seed.k"])
            self.assertIsNone(b.bad)
            b.close()
        finally:
            os.environ.pop("OOTRR_FAKE_STATE", None)
            os.environ.pop("OOTRR_FAKE_SEED_KEY", None)

if __name__ == "__main__":
    unittest.main()
