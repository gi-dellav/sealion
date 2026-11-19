import math


def loop_math(n):
    acc = 0.0
    for i in range(1, n + 1):
        x = float(i)
        acc = acc + math.sqrt(x) + (x * 0.5) / (x + 1.0)
    return acc


if __name__ == "__main__":
    print(int(loop_math(10_000_000)) % 256)
