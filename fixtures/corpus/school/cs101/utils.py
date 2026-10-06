# CS101 homework 3 helpers - Fall semester
# Functions for the number theory problem set. Do not import anything except math.

import math


def is_prime(n):
    """Return True if n is a prime number (trial division up to sqrt(n))."""
    if n < 2:
        return False
    if n % 2 == 0:
        return n == 2
    for d in range(3, int(math.isqrt(n)) + 1, 2):
        if n % d == 0:
            return False
    return True


def fibonacci(k):
    """Return the first k Fibonacci numbers as a list."""
    seq = []
    a, b = 0, 1
    for _ in range(k):
        seq.append(a)
        a, b = b, a + b
    return seq


def gcd(a, b):
    """Euclid's algorithm."""
    while b:
        a, b = b, a % b
    return abs(a)


def collatz_steps(n):
    """Number of steps for n to reach 1 in the Collatz sequence."""
    steps = 0
    while n != 1:
        n = n // 2 if n % 2 == 0 else 3 * n + 1
        steps += 1
    return steps


if __name__ == "__main__":
    print([p for p in range(50) if is_prime(p)])
    print(fibonacci(10))
    print(gcd(84, 36))
    print(collatz_steps(27))  # should print 111
