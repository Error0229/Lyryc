"""Peak-RSS sampler: `with PeakRSS() as m: ...; m.peak_mb`."""
import threading, time, psutil


class PeakRSS:
    def __enter__(self):
        self.proc, self.peak, self._stop = psutil.Process(), 0, False
        self._t = threading.Thread(target=self._run, daemon=True)
        self._t.start()
        return self

    def _run(self):
        while not self._stop:
            self.peak = max(self.peak, self.proc.memory_info().rss)
            time.sleep(0.05)

    def __exit__(self, *exc):
        self._stop = True
        self._t.join()

    @property
    def peak_mb(self):
        return round(self.peak / 2**20)
