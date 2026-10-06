package ratelimit

import (
	"sync"
	"time"
)

// TokenBucket allows bursts up to Capacity and refills at Rate tokens per second.
type TokenBucket struct {
	mu       sync.Mutex
	capacity float64
	rate     float64
	tokens   float64
	last     time.Time
	now      func() time.Time
}

func NewTokenBucket(capacity, ratePerSecond float64) *TokenBucket {
	return &TokenBucket{
		capacity: capacity,
		rate:     ratePerSecond,
		tokens:   capacity,
		last:     time.Now(),
		now:      time.Now,
	}
}

// Allow reports whether one request may proceed right now, consuming a token if so.
func (b *TokenBucket) Allow() bool {
	b.mu.Lock()
	defer b.mu.Unlock()

	now := b.now()
	elapsed := now.Sub(b.last).Seconds()
	b.last = now
	b.tokens += elapsed * b.rate
	if b.tokens > b.capacity {
		b.tokens = b.capacity
	}
	if b.tokens < 1 {
		return false
	}
	b.tokens--
	return true
}

// PerKey keeps one bucket per client key, e.g. per API token or IP address.
type PerKey struct {
	mu      sync.Mutex
	buckets map[string]*TokenBucket
	cap     float64
	rate    float64
}

func NewPerKey(capacity, rate float64) *PerKey {
	return &PerKey{buckets: make(map[string]*TokenBucket), cap: capacity, rate: rate}
}

func (p *PerKey) Allow(key string) bool {
	p.mu.Lock()
	b, ok := p.buckets[key]
	if !ok {
		b = NewTokenBucket(p.cap, p.rate)
		p.buckets[key] = b
	}
	p.mu.Unlock()
	return b.Allow()
}
