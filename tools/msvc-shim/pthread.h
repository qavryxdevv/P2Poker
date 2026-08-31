/* A pthread shim for MSVC, big enough for c-toxcore and no bigger.
 *
 * c-toxcore is C99 and uses pthreads for exactly two things: recursive mutexes
 * and reader-writer locks. On Windows its CMake build pulls in PThreads4W - a
 * whole third library, with its own build, to supply fifteen calls Windows
 * already has native primitives for.
 *
 * This header is the alternative: it is included ahead of the vendored tree and
 * maps those fifteen calls onto the Win32 objects that mean the same thing. It
 * is deliberately not a pthreads implementation. If a future toxcore uses
 * threads, condition variables, or anything else under this name, it will fail
 * to compile here rather than silently link against something that does not do
 * what it says - which is why nothing is stubbed out to a no-op.
 *
 * The exact surface it must cover is `grep -rho "pthread_[a-z_]*"` over
 * `toxcore/` and `toxencryptsave/`.
 *
 * ## Two decisions worth stating
 *
 * **`pthread_mutex_t` is a `CRITICAL_SECTION`.** toxcore asks for
 * `PTHREAD_MUTEX_RECURSIVE` explicitly, and a `CRITICAL_SECTION` is recursive by
 * definition, so the attribute is satisfied by the choice of object rather than
 * by a flag. `SRWLOCK` would have been faster and wrong: it deadlocks the second
 * time one thread takes it.
 *
 * **`pthread_rwlock_t` is also a `CRITICAL_SECTION`, not an `SRWLOCK`.** POSIX
 * has one `pthread_rwlock_unlock` for both modes; Win32 has
 * `ReleaseSRWLockShared` and `ReleaseSRWLockExclusive` and no way to ask a lock
 * which one it is holding. Recording the mode in the lock is wrong the moment
 * two readers hold it at once, and that is exactly the case a reader-writer lock
 * exists for - so the bug would appear only under the concurrency the lock was
 * added to allow.
 *
 * An exclusive lock in both directions is always *correct*; it is only less
 * parallel. toxcore takes these around short list operations in `net_crypto.c`
 * and `mono_time.c`, not around I/O, so the cost is a few nanoseconds of
 * contention and the alternative is a rare, load-dependent crash. If profiling
 * ever shows this to matter, the fix is a real rwlock with a mode-tracking
 * wrapper, written and tested as such - not a guess here.
 */

#ifndef P2P_POKER_MSVC_PTHREAD_SHIM_H
#define P2P_POKER_MSVC_PTHREAD_SHIM_H

#ifndef _WIN32
#error "this shim is for MSVC only; every other platform has real pthreads"
#endif

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <errno.h>

typedef CRITICAL_SECTION pthread_mutex_t;
typedef CRITICAL_SECTION pthread_rwlock_t;

/* toxcore sets exactly one attribute, `PTHREAD_MUTEX_RECURSIVE`, which a
 * CRITICAL_SECTION already is. The type is kept so the calls compile and the
 * value is kept so a future attribute that is *not* satisfied by the object is
 * visible at the call site rather than quietly dropped. */
typedef int pthread_mutexattr_t;

#define PTHREAD_MUTEX_RECURSIVE 1

static __inline int pthread_mutexattr_init(pthread_mutexattr_t *attr)
{
    *attr = 0;
    return 0;
}

static __inline int pthread_mutexattr_settype(pthread_mutexattr_t *attr, int type)
{
    /* Anything but recursive would be a lie: this object is recursive and
     * cannot be made otherwise. Refused rather than ignored. */
    if (type != PTHREAD_MUTEX_RECURSIVE) {
        return EINVAL;
    }

    *attr = type;
    return 0;
}

static __inline int pthread_mutexattr_destroy(pthread_mutexattr_t *attr)
{
    (void)attr;
    return 0;
}

static __inline int pthread_mutex_init(pthread_mutex_t *m, const pthread_mutexattr_t *attr)
{
    (void)attr;
    /* The spin count is the documented default for a lock held briefly: spin
     * before entering the kernel, because every one of these guards a list
     * operation of a few dozen instructions. */
    return InitializeCriticalSectionAndSpinCount(m, 2000) ? 0 : -1;
}

static __inline int pthread_mutex_destroy(pthread_mutex_t *m)
{
    DeleteCriticalSection(m);
    return 0;
}

static __inline int pthread_mutex_lock(pthread_mutex_t *m)
{
    EnterCriticalSection(m);
    return 0;
}

static __inline int pthread_mutex_unlock(pthread_mutex_t *m)
{
    LeaveCriticalSection(m);
    return 0;
}

static __inline int pthread_rwlock_init(pthread_rwlock_t *l, const void *attr)
{
    (void)attr;
    return InitializeCriticalSectionAndSpinCount(l, 2000) ? 0 : -1;
}

static __inline int pthread_rwlock_destroy(pthread_rwlock_t *l)
{
    DeleteCriticalSection(l);
    return 0;
}

static __inline int pthread_rwlock_rdlock(pthread_rwlock_t *l)
{
    EnterCriticalSection(l);
    return 0;
}

static __inline int pthread_rwlock_wrlock(pthread_rwlock_t *l)
{
    EnterCriticalSection(l);
    return 0;
}

static __inline int pthread_rwlock_unlock(pthread_rwlock_t *l)
{
    LeaveCriticalSection(l);
    return 0;
}

#endif /* P2P_POKER_MSVC_PTHREAD_SHIM_H */
