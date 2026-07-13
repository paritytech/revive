/* Knuth Algorithm D remainder for pallet-revive stdlib: computes
 * (hi·2^256 + lo) mod v over base-2^32 digits, following the divmnu
 * structure from Hacker's Delight (2nd ed., 9-2), remainder-only.
 *
 * Digits are 32-bit so every intermediate fits u64 and the quotient-digit
 * estimate is a native 64/64 divide; no operation wider than the RV64
 * hardware is left for LLVM to expand.
 *
 * u: 16 little-endian u32 limbs (512-bit dividend)
 * v:  8 little-endian u32 limbs (256-bit divisor), must be nonzero
 * r:  8 little-endian u32 limbs (remainder out)
 */

typedef unsigned int u32;
typedef unsigned long long u64;
typedef long long i64;

#define B32 0x100000000ULL

static inline int nlz32(u32 x) {
    /* clang maps this to a clz sequence; defined here to avoid libcalls */
    int n = 0;
    if (x == 0) return 32;
    if (x <= 0x0000FFFF) { n += 16; x <<= 16; }
    if (x <= 0x00FFFFFF) { n += 8;  x <<= 8;  }
    if (x <= 0x0FFFFFFF) { n += 4;  x <<= 4;  }
    if (x <= 0x3FFFFFFF) { n += 2;  x <<= 2;  }
    if (x <= 0x7FFFFFFF) { n += 1; }
    return n;
}

void __ulongrem_knuth(const u32 *u, const u32 *v, u32 *r) {
    u32 un[17], vn[8];
    u64 qhat, rhat, p, top;
    i64 t, k;
    int s, i, j, n, m;

    /* significant limb counts */
    n = 8;
    while (n > 1 && v[n - 1] == 0) n--;
    m = 16;
    while (m > 1 && u[m - 1] == 0) m--;

    /* u < v by limb count: remainder is u */
    if (m < n) {
        for (i = 0; i < 8; i++) r[i] = i < m ? u[i] : 0;
        return;
    }

    /* single-limb divisor: straight 64/32 remainder scan */
    if (n == 1) {
        u64 rem = 0;
        for (i = m - 1; i >= 0; i--) {
            rem = ((rem << 32) | u[i]) % v[0];
        }
        r[0] = (u32)rem;
        for (i = 1; i < 8; i++) r[i] = 0;
        return;
    }

    /* D1: normalize so vn[n-1] has its top bit set. The (32 - s) right
     * shifts are written as (31 - s) then 1 to stay defined at s == 0. */
    s = nlz32(v[n - 1]);
    for (i = n - 1; i > 0; i--)
        vn[i] = (u32)((v[i] << s) | ((u64)v[i - 1] >> (31 - s) >> 1));
    vn[0] = v[0] << s;

    un[m] = (u32)((u64)u[m - 1] >> (31 - s) >> 1);
    for (i = m - 1; i > 0; i--)
        un[i] = (u32)((u[i] << s) | ((u64)u[i - 1] >> (31 - s) >> 1));
    un[0] = u[0] << s;

    /* D2..D7: one base-2^32 quotient digit per iteration (discarded;
     * only the running remainder in un[] matters). */
    for (j = m - n; j >= 0; j--) {
        /* D3: estimate the digit from the top two remainder limbs */
        top = ((u64)un[j + n] << 32) | un[j + n - 1];
        qhat = top / vn[n - 1];
        rhat = top - qhat * vn[n - 1];
        while (qhat >= B32 ||
               qhat * vn[n - 2] > ((rhat << 32) | un[j + n - 2])) {
            qhat--;
            rhat += vn[n - 1];
            if (rhat >= B32) break;
        }

        /* D4: multiply and subtract */
        k = 0;
        for (i = 0; i < n; i++) {
            p = qhat * vn[i];
            t = (i64)un[i + j] - k - (i64)(p & 0xFFFFFFFFULL);
            un[i + j] = (u32)t;
            k = (i64)(p >> 32) - (t >> 32);
        }
        t = (i64)un[j + n] - k;
        un[j + n] = (u32)t;

        /* D5/D6: add back on the rare overestimate */
        if (t < 0) {
            k = 0;
            for (i = 0; i < n; i++) {
                t = (u64)un[i + j] + vn[i] + k;
                un[i + j] = (u32)t;
                k = t >> 32;
            }
            un[j + n] += (u32)k;
        }
    }

    /* D8: denormalize the remainder */
    for (i = 0; i < n - 1; i++)
        r[i] = (u32)((un[i] >> s) | ((u64)un[i + 1] << (31 - s) << 1));
    r[n - 1] = un[n - 1] >> s;
    for (i = n; i < 8; i++) r[i] = 0;
}
