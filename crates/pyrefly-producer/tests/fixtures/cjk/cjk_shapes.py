"""CJK docstring — 寬字元素材（共、會）：char-boundary safety for emit."""

# 中文註解行——刻意放在定義之前，讓 byte/char 漂移累積在定義前方。


class Greeter:
    """問候器——docstring 含中文。"""

    def greet(self, name: str) -> str:
        # 回傳問候語（繁體中文）
        return f"你好，{name}"


def make_greeter() -> Greeter:
    """工廠函式——回傳 Greeter 實例（建構子呼叫）。"""
    return Greeter()
