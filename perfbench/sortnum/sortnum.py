MASK = 0xFFFFFFFF


def fill(n):
    a = []
    x = 2463534242
    for _ in range(n):
        x = (x ^ (x << 13)) & MASK
        x = (x ^ (x >> 17)) & MASK
        x = (x ^ (x << 5)) & MASK
        a.append((x >> 8) % 1000)
    return a


def fold(a, n):
    acc = 0
    for i in range(n):
        acc = (acc + (a[i] % 256) * ((i % 7) + 1)) % 65536
    return acc


if __name__ == "__main__":
    n = 200_000
    a = fill(n)
    a.sort()
    print(fold(a, n) % 256)
