"""Пример плагина синтеза речи для Звукопада.

Это шаблон: вместо речи он создаёт писк, длина которого зависит от длины текста.
Чтобы сделать настоящий плагин, замените функцию synthesize() на вызов своей нейросети
(Piper, Silero, XTTS и т. д.) — она должна записать WAV-файл по пути output.

Протокол: Звукопад пишет в stdin по одной строке JSON на запрос, плагин отвечает
одной строкой JSON в stdout. Всё лишнее (отладка, прогресс) — только в stderr.
"""

import json
import math
import struct
import sys
import wave


def log(*args):
    print(*args, file=sys.stderr, flush=True)


def list_voices():
    return {"voices": [
        {"id": "beep-high", "name": "Писк высокий", "language": "ru-RU"},
        {"id": "beep-low", "name": "Писк низкий", "language": "ru-RU"},
    ]}


def synthesize(text, voice, rate, output):
    sample_rate = 22050
    freq = 880.0 if voice != "beep-low" else 330.0
    seconds = min(0.08 * max(len(text), 1) * (1.0 - rate * 0.05), 20.0)
    frames = int(sample_rate * seconds)
    with wave.open(output, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sample_rate)
        data = bytearray()
        for i in range(frames):
            fade = min(1.0, i / 500, (frames - i) / 500)
            sample = math.sin(2 * math.pi * freq * i / sample_rate) * 0.3 * fade
            data += struct.pack("<h", int(sample * 32767))
        w.writeframes(bytes(data))
    return {"path": output}


def handle(method, params):
    if method == "info":
        return {"name": "Python example", "protocol": 1}
    if method == "list_voices":
        return list_voices()
    if method == "synthesize":
        return synthesize(params.get("text", ""), params.get("voice", ""), int(params.get("rate", 0)), params["output"])
    raise ValueError(f"неизвестный метод: {method}")


def main():
    log("python-example: запущен")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        req = json.loads(line)
        if req.get("method") == "shutdown":
            break
        try:
            reply = {"id": req.get("id"), "result": handle(req.get("method"), req.get("params") or {})}
        except Exception as e:  # ошибка уходит в программу и показывается пользователю
            reply = {"id": req.get("id"), "error": {"message": str(e)}}
        # ensure_ascii=True (по умолчанию) — кодировка консоли не важна
        sys.stdout.write(json.dumps(reply) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
