"""K-means clustering from scratch with numpy, using k-means++ initialization."""

import numpy as np


def kmeans_pp_init(X: np.ndarray, k: int, rng: np.random.Generator) -> np.ndarray:
    centers = [X[rng.integers(len(X))]]
    for _ in range(1, k):
        d2 = np.min(((X[:, None, :] - np.array(centers)[None]) ** 2).sum(-1), axis=1)
        probs = d2 / d2.sum()
        centers.append(X[rng.choice(len(X), p=probs)])
    return np.array(centers)


def kmeans(X: np.ndarray, k: int, max_iter: int = 100, tol: float = 1e-6, seed: int = 0):
    """Return (centroids, labels, inertia)."""
    rng = np.random.default_rng(seed)
    centroids = kmeans_pp_init(X, k, rng)
    for _ in range(max_iter):
        dists = ((X[:, None, :] - centroids[None]) ** 2).sum(-1)
        labels = dists.argmin(axis=1)
        new_centroids = np.array([
            X[labels == j].mean(axis=0) if np.any(labels == j) else centroids[j]
            for j in range(k)
        ])
        shift = np.linalg.norm(new_centroids - centroids)
        centroids = new_centroids
        if shift < tol:
            break
    inertia = float(((X - centroids[labels]) ** 2).sum())
    return centroids, labels, inertia


def elbow(X: np.ndarray, k_max: int = 8) -> list[float]:
    """Inertia for k = 1..k_max, to eyeball the elbow."""
    return [kmeans(X, k)[2] for k in range(1, k_max + 1)]


if __name__ == "__main__":
    rng = np.random.default_rng(42)
    blobs = np.vstack([
        rng.normal(loc, 0.5, size=(100, 2)) for loc in ([0, 0], [5, 5], [0, 6])
    ])
    c, labels, inertia = kmeans(blobs, 3)
    print("centroids:\n", c.round(2))
    print("inertia:", round(inertia, 2))
