MASK = 0xFFFFFFFF


def xorshift(n):
    x = 2463534242
    acc = 0
    for _ in range(n):
        x = (x ^ (x << 13)) & MASK
        x = (x ^ (x >> 17)) & MASK
        x = (x ^ (x << 5)) & MASK
        acc = (acc + x) % 65536
    return acc


if __name__ == "__main__":
    print(xorshift(1_000_000) % 256)
