"""Length of the YouTube upload a raw player title names: one YouTube search per
title, the result whose title is exactly the raw title, else the top three to pick by hand. Sequential, 2 s apart.
Usage: python -X utf8 yt_lengths.py "raw title" ...  -> JSON {title: seconds | [[title, seconds], ...]}"""
import json, re, sys, time, urllib.parse, urllib.request

UA = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)", "Accept-Language": "en"}
ITEM = re.compile(
    r'"title":\{"runs":\[\{"text":"((?:[^"\\]|\\.)*)"\}\],"accessibility":\{"accessibilityData":\{"label":"'
    r'[^"]*?(?:(\d+) hours?, )?(?:(\d+) minutes?, )?(\d+) seconds?"\}\}\}'
)


def length(title):
    url = "https://www.youtube.com/results?search_query=" + urllib.parse.quote(title)
    html = urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=20).read().decode("utf-8")
    found = []
    for m in ITEM.finditer(html):
        h, mi, s = (int(g or 0) for g in m.groups()[1:])
        found.append((json.loads('"' + m.group(1) + '"').strip(), h * 3600 + mi * 60 + s))
    exact = [secs for name, secs in found if name == title.strip()]
    # No exact title: the top results, to pick by hand.
    return exact[0] if exact else found[:3]


out = {}
for t in sys.argv[1:]:
    out[t] = length(t)
    time.sleep(2)
print(json.dumps(out, ensure_ascii=False, indent=1))
