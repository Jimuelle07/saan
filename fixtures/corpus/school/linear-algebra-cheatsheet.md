# Linear algebra cheat sheet (midterm 2)

## Determinants
- det(AB) = det(A) det(B); det(A^T) = det(A); det(A^-1) = 1/det(A)
- Swapping two rows flips the sign; scaling a row by k scales det by k.
- A is invertible <=> det(A) != 0.

## Eigenvalues and eigenvectors
- Solve det(A - lambda I) = 0 for eigenvalues; then null space of (A - lambda I) for eigenvectors.
- Sum of eigenvalues = trace(A); product = det(A).
- A is diagonalizable if it has n linearly independent eigenvectors: A = P D P^-1.
- Symmetric matrices: real eigenvalues, orthogonal eigenvectors (spectral theorem).

## Rank-nullity
- rank(A) + nullity(A) = number of columns.

## Orthogonality
- Projection of b onto a: (a.b / a.a) a
- Gram-Schmidt turns a basis into an orthonormal basis.
- Least squares: solve A^T A x = A^T b.

## Singular value decomposition
- A = U Sigma V^T; singular values are square roots of eigenvalues of A^T A.
- Best rank-k approximation: keep the k largest singular values.

## Common mistakes
- Matrix multiplication is not commutative.
- Eigenvectors are never the zero vector.
