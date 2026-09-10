//! Where everything on the table goes.
//!
//! Pure geometry: no painting, no `Ui`, no state. That is deliberate — the three
//! things that were asked for are all statements about rectangles, and a
//! statement about rectangles can be tested without a window:
//!
//! * **the hero is always at the bottom**, whatever seat they were dealt;
//! * **a bet is never covered** by a card, a plate or the pot;
//! * **everything is aligned** — inside the window, not on top of each other.
//!
//! Every one of those is a test below, run over every seat count from two to
//! ten, every hero seat, and window sizes from the smallest the client allows to
//! a large one. A layout that only works at one size works by accident.
//!
//! # The seat ring
//!
//! Seats sit on an ellipse. The hero is put at the bottom of it and the rest
//! follow **clockwise on screen**, which is how the reference photograph is laid
//! out and how a player reads the order of action: the seat after the hero is on
//! the hero's left as drawn.
//!
//! Screen coordinates have `y` growing downward, so the bottom of the ellipse is
//! at angle `π/2` and increasing the angle moves left — clockwise on screen. The
//! sign convention is worth stating because getting it wrong produces a table
//! that looks fine and deals in the wrong direction.
//!
//! # Why things are placed by *support distance* and not by a fixed offset
//!
//! A plate is much wider than it is tall. Offsetting a card "sixty pixels toward
//! the middle" clears the plate at the top seat and lands in the middle of it at
//! the side seat. So every offset here is measured with [`support`], the extent
//! of a rectangle in the direction actually being moved. That is the difference
//! between a layout that holds for six seats and one that holds for any number.

use std::f32::consts::{FRAC_PI_2, TAU};

use eframe::egui::{pos2, vec2, Pos2, Rect, Vec2};

/// The proportions, in one place, because they are the thing that gets tuned.
mod ratio {
    /// The name plate's height, as a fraction of the drawing area's height.
    pub const PLATE_H: f32 = 0.072;
    pub const PLATE_H_MIN: f32 = 22.0;
    pub const PLATE_H_MAX: f32 = 50.0;
    /// The plate's width, as a multiple of its height.
    pub const PLATE_ASPECT: f32 = 2.7;
    /// The portrait disc, as a multiple of the plate's height.
    pub const AVATAR: f32 = 0.9;
    /// The wooden rail, as a fraction of the felt's smaller radius.
    pub const RAIL: f32 = 0.098;
    pub const RAIL_MIN: f32 = 7.0;
    pub const RAIL_MAX: f32 = 28.0;
    /// A board card's width, against the felt's width and its height. The
    /// smaller of the two wins, so a short window shrinks the cards rather than
    /// letting them run off the felt.
    pub const CARD_W_OF_FELT_W: f32 = 0.076;
    pub const CARD_W_OF_FELT_H: f32 = 0.104;
    pub const CARD_ASPECT: f32 = 1.4;
    /// The hero's cards are bigger than the board's and an opponent's smaller —
    /// the reference does this, and it is what makes the hero's own hand
    /// readable without looking for it.
    pub const HERO_CARD: f32 = 1.12;
    pub const SEAT_CARD: f32 = 0.56;
    /// A bet marker: a chip and its amount beside it.
    pub const BET_W: f32 = 1.15;
    pub const BET_H: f32 = 0.34;
    /// The dealer button, as a multiple of the plate's height.
    pub const BUTTON: f32 = 0.55;
    /// The breathing space between two things that must not touch.
    pub const GAP: f32 = 0.13;
}

/// How far in from a seat its chips are drawn, as a fraction of the way to the
/// middle. The same preferred number for every seat, which is most of what keeps
/// two seats' chips from meeting.
const BET_RING: f32 = 0.62;

/// How much the middle of the table gives up per attempt when some seat has no
/// room for its chips, and how many attempts it gets.
const SQUEEZE: f32 = 0.05;
const SQUEEZE_STEPS: u8 = 8;

/// Every size the table is drawn from, derived from the area it is given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub plate: Vec2,
    pub avatar: f32,
    pub rail: f32,
    pub board_card: Vec2,
    pub hero_card: Vec2,
    pub seat_card: Vec2,
    pub bet: Vec2,
    pub button: f32,
    pub gap: f32,
}

impl Metrics {
    fn new(area: Rect) -> Self {
        let plate_h =
            (area.height() * ratio::PLATE_H).clamp(ratio::PLATE_H_MIN, ratio::PLATE_H_MAX);
        Metrics {
            plate: vec2(plate_h * ratio::PLATE_ASPECT, plate_h),
            avatar: plate_h * ratio::AVATAR,
            // The rest is filled in by `with_felt`: the card sizes depend on the
            // felt and the felt depends on the plate, so it cannot be one pass.
            rail: 0.0,
            board_card: Vec2::ZERO,
            hero_card: Vec2::ZERO,
            seat_card: Vec2::ZERO,
            bet: Vec2::ZERO,
            button: 0.0,
            gap: 0.0,
        }
    }

    /// The sizes that depend on the felt.
    ///
    /// `scale` shrinks everything drawn **on** the felt — the cards, the chips,
    /// the spacing — without touching the plates, which belong to the rail.
    /// [`Layout::new`] turns it down when a table is too tight to give every
    /// seat's chips a line of their own; the comment there says why.
    fn with_felt(mut self, felt: Rect, scale: f32) -> Self {
        let w = (felt.width() * ratio::CARD_W_OF_FELT_W)
            .min(felt.height() * ratio::CARD_W_OF_FELT_H)
            * scale;
        self.rail = (felt.height().min(felt.width()) * 0.5 * ratio::RAIL)
            .clamp(ratio::RAIL_MIN, ratio::RAIL_MAX);
        self.board_card = vec2(w, w * ratio::CARD_ASPECT);
        self.hero_card = self.board_card * ratio::HERO_CARD;
        self.seat_card = self.board_card * ratio::SEAT_CARD;
        self.bet = vec2(w * ratio::BET_W, w * ratio::BET_H);
        // Sized against the plate it rides on, not against a card: a button
        // scaled to the middle of the table grows past its own plate on a wide
        // window and lands back on the number it was moved off.
        self.button = self.plate.y * ratio::BUTTON;
        self.gap = w * ratio::GAP;
        self
    }
}

/// One seat's boxes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeatSlot {
    pub seat: u8,
    pub is_hero: bool,
    /// The name and the stack.
    pub plate: Rect,
    /// The portrait disc, above the plate.
    pub avatar: Rect,
    /// The two hole cards, toward the middle of the table.
    pub cards: [Rect; 2],
    /// The chips this seat has put in, further toward the middle still.
    pub bet: Rect,
    /// The dealer button, on the inner edge of the plate. Drawn only for the
    /// seat that has it.
    pub button: Rect,
    /// Outward unit vector, from the middle of the felt to this seat.
    pub out: Vec2,
}

impl SeatSlot {
    /// The boxes that must never be covered by another seat's boxes.
    #[cfg(test)]
    fn solids(&self) -> [Rect; 4] {
        [self.plate, self.avatar, self.cards[0], self.cards[1]]
    }
}

/// The whole table, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub area: Rect,
    /// The outer edge of the wooden rail.
    pub rail: Rect,
    /// The green.
    pub felt: Rect,
    pub centre: Pos2,
    pub board: [Rect; 5],
    pub pot: Rect,
    pub seats: Vec<SeatSlot>,
    pub metrics: Metrics,
}

/// How far a rectangle reaches from `from` in the direction `dir`.
///
/// The support function of the rectangle, which is what "how much room does this
/// take up *that way*" means for a box that is not square. Using it instead of a
/// fixed offset is why a card clears a plate at every angle and not only at the
/// top of the table.
pub fn support(rect: Rect, from: Pos2, dir: Vec2) -> f32 {
    let c = rect.center() - from;
    c.dot(dir) + dir.x.abs() * rect.width() * 0.5 + dir.y.abs() * rect.height() * 0.5
}

/// The point on an ellipse at `angle`.
fn on_ellipse(centre: Pos2, radius: Vec2, angle: f32) -> Pos2 {
    pos2(
        centre.x + radius.x * angle.cos(),
        centre.y + radius.y * angle.sin(),
    )
}

/// The smallest distance from the middle, along `dir`, at which a box of
/// half-size `half` is completely outside a box of half-size `zone`.
///
/// Both boxes are centred on the middle of the felt. A rectangle centred at
/// `t * dir` overlaps the zone only while it is inside it on **both** axes, so
/// escaping either one is enough and the answer is the smaller of the two.
fn escape(dir: Vec2, zone: Vec2, half: Vec2) -> f32 {
    let axis = |d: f32, f: f32| {
        if d.abs() > 1e-4 {
            f / d.abs()
        } else {
            f32::INFINITY
        }
    };
    axis(dir.x, zone.x + half.x).min(axis(dir.y, zone.y + half.y))
}

/// One seat, before the chips are placed. The chips are the only thing on the
/// table whose position depends on more than its own seat, so they come second.
struct Half {
    seat: u8,
    is_hero: bool,
    anchor: Pos2,
    out: Vec2,
    plate: Rect,
    avatar: Rect,
    cards: [Rect; 2],
    /// How close to the middle this seat's cards reach.
    inner: f32,
}

/// The board, the pot, and the half-extents a bet must stay outside of.
///
/// The forbidden region is made symmetric about the middle so one pair of
/// numbers answers for every direction. It over-reserves a little below the
/// board, which is the safe direction to be wrong in.
fn middle(m: &Metrics, centre: Pos2) -> ([Rect; 5], Rect, Vec2) {
    let card = m.board_card;
    let step = card.x + m.gap;
    let board_y = centre.y + card.y * 0.06;
    let first = centre.x - step * 2.0;
    let board: [Rect; 5] = std::array::from_fn(|i| {
        Rect::from_center_size(pos2(first + step * i as f32, board_y), card)
    });
    let pot_size = vec2(card.x * 3.4, card.y * 0.36);
    let pot = Rect::from_center_size(
        pos2(centre.x, board[0].top() - pot_size.y * 0.5 - m.gap * 1.4),
        pot_size,
    );
    let zone = vec2(
        (board[4].right() - centre.x).abs() + m.gap,
        (board[0].bottom() - centre.y)
            .abs()
            .max((pot.top() - centre.y).abs())
            + m.gap,
    );
    (board, pot, zone)
}

/// Every seat's plate, portrait and cards.
fn seat_halves(m: &Metrics, centre: Pos2, ring: Vec2, n: u8, hero: u8) -> Vec<Half> {
    (0..n)
        .map(|seat| {
            // The hero at the bottom; the rest clockwise on screen from there,
            // which — with `y` growing downward — is the direction the angle
            // increases in.
            let round = ((seat + n - hero) % n) as f32 / n as f32;
            let anchor = on_ellipse(centre, ring, FRAC_PI_2 + TAU * round);
            let out = (anchor - centre).normalized();
            let is_hero = seat == hero;

            let plate = Rect::from_center_size(anchor, m.plate);
            let avatar = Rect::from_center_size(
                pos2(plate.center().x, plate.top() - m.avatar * 0.5 - 2.0),
                Vec2::splat(m.avatar),
            );

            // The cards go toward the middle, clear of the plate and of the
            // portrait above it — measured with the support function, so it
            // holds at the side seats as well as at the top.
            let inward = -out;
            let head = plate.union(avatar);
            let cs = if is_hero { m.hero_card } else { m.seat_card };
            let pair = Rect::from_center_size(Pos2::ZERO, vec2(cs.x * 2.0 + m.gap * 0.5, cs.y));
            let d_cards = support(head, anchor, inward) + support(pair, Pos2::ZERO, inward) + m.gap;
            let cards_at = anchor + inward * d_cards;
            let apart = (cs.x + m.gap * 0.25) * 0.5;

            Half {
                seat,
                is_hero,
                anchor,
                out,
                plate,
                avatar,
                cards: [
                    Rect::from_center_size(pos2(cards_at.x - apart, cards_at.y), cs),
                    Rect::from_center_size(pos2(cards_at.x + apart, cards_at.y), cs),
                ],
                inner: (anchor - centre).length() - d_cards - support(pair, Pos2::ZERO, inward),
            }
        })
        .collect()
}

/// The nearest and furthest a seat may put its chips, on its own line.
fn chip_range(m: &Metrics, zone: Vec2, h: &Half) -> (f32, f32) {
    let chip = Rect::from_center_size(Pos2::ZERO, m.bet);
    let floor = escape(h.out, zone, m.bet * 0.5) + 1.0;
    let ceiling = h.inner - support(chip, Pos2::ZERO, -h.out) - m.gap;
    (floor, ceiling)
}

impl Layout {
    /// Resolve the table into rectangles.
    ///
    /// `hero` is the seat the local player occupies; it is put at the bottom and
    /// everything else follows from that.
    pub fn new(area: Rect, seats: u8, hero: u8) -> Layout {
        let n = seats.clamp(2, 10);
        let hero = hero % n;
        let base = Metrics::new(area);

        // The felt is what is left after room is made for a plate on every edge
        // — and, at the top, for the portrait that sits above the plate.
        let inner = area.shrink(6.0);
        let rail_guess = (inner.height().min(inner.width()) * 0.5 * ratio::RAIL)
            .clamp(ratio::RAIL_MIN, ratio::RAIL_MAX);
        let band_x = rail_guess * 0.5 + base.plate.x * 0.5;
        let band_top = rail_guess * 0.5 + base.plate.y * 0.5 + base.avatar + 6.0;
        let band_bottom = rail_guess * 0.5 + base.plate.y * 0.5 + 6.0;
        let mut felt = Rect::from_min_max(
            pos2(inner.left() + band_x, inner.top() + band_top),
            pos2(inner.right() - band_x, inner.bottom() - band_bottom),
        );

        // A poker table is wider than it is deep. Without this a tall window
        // produces a circle, which is not what anyone has played on -- and
        // at 1.7 the ordinary window still did (`S1-CS`: *the table is round
        // where it should be an oval*). Two to one is the proportion of the
        // reference photograph, and the felt keeps it at every window size,
        // following the width.
        let max_h = felt.width() / 2.0;
        if felt.height() > max_h {
            felt = Rect::from_center_size(felt.center(), vec2(felt.width(), max_h));
        }
        let centre = felt.center();

        // Everything drawn on the felt is sized from one number, and that number
        // is turned down until every seat's chips have somewhere to go.
        //
        // A tight table — five seats on the smallest window the client allows —
        // can genuinely have no room on a seat's line between the edge of the
        // board and the near edge of that seat's own cards. Two earlier attempts
        // nudged the offending bet sideways instead, and it landed on the *next*
        // seat's cards: a bet moved off its own line is in nobody's territory,
        // and no per-seat calculation can see that coming. Shrinking the middle
        // of the table by a few per cent costs nothing anyone can see and gives
        // every seat its line back, so that is what happens instead.
        let mut m = base.with_felt(felt, 1.0);
        let mut ring = Vec2::ZERO;
        let mut board = [Rect::ZERO; 5];
        let mut pot = Rect::ZERO;
        let mut zone = Vec2::ZERO;
        let mut halves: Vec<Half> = Vec::new();

        for attempt in 0..=SQUEEZE_STEPS {
            m = base.with_felt(felt, 1.0 - SQUEEZE * attempt as f32);
            ring = vec2(
                felt.width() * 0.5 + m.rail * 0.5,
                felt.height() * 0.5 + m.rail * 0.5,
            );
            let (b, p, z) = middle(&m, centre);
            board = b;
            pot = p;
            zone = z;
            halves = seat_halves(&m, centre, ring, n, hero);
            let tight = halves.iter().any(|h| {
                let (floor, ceiling) = chip_range(&m, zone, h);
                ceiling < floor
            });
            if !tight {
                break;
            }
        }
        let _ = ring;

        // Chips sit on the line from the seat to the middle, at the same
        // preferred fraction of the way in for every seat, moved only as far as
        // that seat needs: outward so the board is not underneath, inward so the
        // seat's own cards are not on top.
        let seats = halves
            .into_iter()
            .map(|h| {
                let reach = (h.anchor - centre).length();
                let inward = -h.out;
                let (floor, ceiling) = chip_range(&m, zone, &h);
                let bet = Rect::from_center_size(
                    centre + h.out * (BET_RING * reach).clamp(floor, floor.max(ceiling)),
                    m.bet,
                );

                // The dealer button rides on the inner edge of the plate, mostly
                // The dealer button rides in the **corner** of its own plate.
                //
                // On the plate because anywhere out on the felt it becomes one
                // more thing that can land on a neighbour, and on the plate it
                // inherits the plate's guarantee of having that space to
                // itself. In the corner because the middle of the inner edge is
                // where the stack is written, and the first version put a large
                // white D through the middle of the number.
                let tangent = vec2(-h.out.y, h.out.x);
                let button = Rect::from_center_size(
                    h.anchor
                        + inward * (support(h.plate, h.anchor, inward) - m.button * 0.5 - 1.0)
                        + tangent * (support(h.plate, h.anchor, tangent) - m.button * 0.5 - 1.0),
                    Vec2::splat(m.button),
                );

                SeatSlot {
                    seat: h.seat,
                    is_hero: h.is_hero,
                    plate: h.plate,
                    avatar: h.avatar,
                    cards: h.cards,
                    bet,
                    button,
                    out: h.out,
                }
            })
            .collect();

        Layout {
            area,
            rail: felt.expand(m.rail),
            felt,
            centre,
            board,
            pot,
            seats,
            metrics: m,
        }
    }

    /// The hero's slot, which always exists.
    pub fn hero(&self) -> &SeatSlot {
        self.seats
            .iter()
            .find(|s| s.is_hero)
            .expect("one seat is always the hero")
    }

    /// The slot for a seat index, if the table has one.
    pub fn seat(&self, seat: u8) -> Option<&SeatSlot> {
        self.seats.iter().find(|s| s.seat == seat)
    }

    /// Every card rectangle on the table.
    pub fn all_cards(&self) -> Vec<Rect> {
        let mut v: Vec<Rect> = self.board.to_vec();
        for s in &self.seats {
            v.extend_from_slice(&s.cards);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sizes a real window can have: the smallest the client permits, the
    /// default, and larger. A layout that only holds at one size holds by
    /// accident.
    const SIZES: [(f32, f32); 5] = [
        (720.0, 390.0),
        (980.0, 570.0),
        (1280.0, 700.0),
        (1920.0, 980.0),
        (2560.0, 1320.0),
    ];

    fn every_layout(mut f: impl FnMut(&Layout, u8, u8)) {
        for (w, h) in SIZES {
            let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
            for n in 2..=10u8 {
                for hero in 0..n {
                    f(&Layout::new(area, n, hero), n, hero);
                }
            }
        }
    }

    /// The thing that was asked for first: wherever the hero was seated, they
    /// are drawn at the bottom, in the middle.
    #[test]
    fn the_hero_is_always_at_the_bottom() {
        every_layout(|l, n, hero| {
            let h = l.hero();
            assert_eq!(h.seat, hero);
            for other in &l.seats {
                if other.seat != hero {
                    assert!(
                        h.plate.center().y >= other.plate.center().y - 0.5,
                        "{n} seats, hero {hero}: someone is drawn lower than the hero"
                    );
                }
            }
            assert!(
                (h.plate.center().x - l.centre.x).abs() < 1.0,
                "{n} seats, hero {hero}: the hero is not centred"
            );
        });
    }

    /// The second thing: a bet is never hidden under a card.
    #[test]
    fn no_card_ever_covers_a_bet() {
        every_layout(|l, n, hero| {
            let cards = l.all_cards();
            for s in &l.seats {
                for card in &cards {
                    assert!(
                        !s.bet.intersects(*card),
                        "{n} seats, hero {hero}: seat {}'s bet is under a card",
                        s.seat
                    );
                }
            }
        });
    }

    /// Nor under the pot, a plate, a dealer button or another seat's chips — a
    /// bet that is covered is a bet the player cannot read, whatever is on top.
    #[test]
    fn nothing_else_covers_a_bet_either() {
        every_layout(|l, n, hero| {
            for s in &l.seats {
                assert!(
                    !s.bet.intersects(l.pot),
                    "{n} seats, hero {hero}: seat {}'s bet is under the pot",
                    s.seat
                );
                for other in &l.seats {
                    assert!(
                        !s.bet.intersects(other.plate),
                        "{n}/{hero}: seat {}'s bet is under seat {}'s plate",
                        s.seat,
                        other.seat
                    );
                    assert!(
                        !s.bet.intersects(other.button),
                        "{n}/{hero}: seat {}'s bet is under a dealer button",
                        s.seat
                    );
                    if other.seat != s.seat {
                        assert!(
                            !s.bet.intersects(other.bet),
                            "{n}/{hero}: seats {} and {} share a bet marker",
                            s.seat,
                            other.seat
                        );
                    }
                }
            }
        });
    }

    /// The third thing: alignment. Nothing is drawn outside the window.
    #[test]
    fn everything_stays_inside_the_area() {
        every_layout(|l, n, hero| {
            let inside = l.area.expand(0.5);
            let check = |r: Rect, what: &str| {
                assert!(
                    inside.contains_rect(r),
                    "{n} seats, hero {hero}: {what} is outside the window ({r:?} vs {:?})",
                    l.area
                );
            };
            check(l.rail, "the rail");
            check(l.pot, "the pot");
            for (i, c) in l.board.iter().enumerate() {
                check(*c, &format!("board card {i}"));
            }
            for s in &l.seats {
                check(s.plate, &format!("seat {}'s plate", s.seat));
                check(s.avatar, &format!("seat {}'s portrait", s.seat));
                check(s.cards[0], &format!("seat {}'s first card", s.seat));
                check(s.cards[1], &format!("seat {}'s second card", s.seat));
                check(s.bet, &format!("seat {}'s bet", s.seat));
            }
        });
    }

    /// A seat's own boxes are a stack, not a pile: the plate, the portrait and
    /// the cards each have their own space.
    #[test]
    fn a_seat_does_not_overlap_itself() {
        every_layout(|l, n, hero| {
            for s in &l.seats {
                assert!(
                    !s.cards[0].intersects(s.plate) && !s.cards[1].intersects(s.plate),
                    "{n}/{hero}: seat {}'s cards cover its name",
                    s.seat
                );
                assert!(
                    !s.cards[0].intersects(s.avatar) && !s.cards[1].intersects(s.avatar),
                    "{n}/{hero}: seat {}'s cards cover its portrait",
                    s.seat
                );
                assert!(
                    !s.cards[0].intersects(s.cards[1]),
                    "{n}/{hero}: seat {}'s two cards are on top of each other",
                    s.seat
                );
            }
        });
    }

    /// And two seats do not overlap each other. Ten seats on one ellipse is the
    /// case that fails first, which is why the range goes that far.
    #[test]
    fn two_seats_never_overlap() {
        every_layout(|l, n, hero| {
            for (i, a) in l.seats.iter().enumerate() {
                for b in &l.seats[i + 1..] {
                    for x in a.solids() {
                        for y in b.solids() {
                            assert!(
                                !x.intersects(y),
                                "{n} seats, hero {hero}: seat {} overlaps seat {}",
                                a.seat,
                                b.seat
                            );
                        }
                    }
                }
            }
        });
    }

    /// The dealer button rides on its own plate and touches nothing else. That
    /// is the whole reason it was put there rather than out on the felt.
    #[test]
    fn the_dealer_button_stays_on_its_own_plate() {
        every_layout(|l, n, hero| {
            for s in &l.seats {
                for other in &l.seats {
                    if other.seat != s.seat {
                        assert!(
                            !s.button.intersects(other.plate),
                            "{n}/{hero}: seat {}'s button is on seat {}'s plate",
                            s.seat,
                            other.seat
                        );
                    }
                    for c in other.cards {
                        assert!(
                            !s.button.intersects(c),
                            "{n}/{hero}: seat {}'s button is on a card",
                            s.seat
                        );
                    }
                }
            }
        });
    }

    /// The dealer button stays out of the middle of its own plate, which is
    /// where the stack is written. The first version drew a large white D
    /// straight through the number.
    #[test]
    fn the_dealer_button_keeps_off_the_stack() {
        every_layout(|l, n, hero| {
            for s in &l.seats {
                let numbers = Rect::from_center_size(
                    s.plate.center(),
                    vec2(s.plate.width() * 0.5, s.plate.height() * 0.9),
                );
                assert!(
                    !s.button.intersects(numbers),
                    "{n}/{hero}: seat {}'s button is over its own name and stack",
                    s.seat
                );
            }
        });
    }

    /// The hero's cards are the largest on the table. Theirs is the one hand
    /// that has to be read without looking for it.
    #[test]
    fn the_heros_cards_are_the_biggest() {
        every_layout(|l, n, hero| {
            assert!(
                l.metrics.hero_card.x > l.metrics.board_card.x,
                "{n}/{hero}: the hero's cards are no bigger than the board's"
            );
            assert!(l.metrics.board_card.x > l.metrics.seat_card.x);
        });
    }

    /// No hole card is drawn over the board or the pot; the middle of the table
    /// belongs to the hand.
    #[test]
    fn the_middle_of_the_table_is_left_alone() {
        every_layout(|l, n, hero| {
            for s in &l.seats {
                for c in s.cards {
                    for b in l.board {
                        assert!(
                            !c.intersects(b),
                            "{n}/{hero}: seat {}'s card is over the board",
                            s.seat
                        );
                    }
                    assert!(
                        !c.intersects(l.pot),
                        "{n}/{hero}: seat {}'s card is over the pot",
                        s.seat
                    );
                }
            }
        });
    }

    /// The board is five cards, evenly spaced, centred on the felt — the
    /// symmetry the eye uses to find the middle of the table.
    #[test]
    fn the_board_is_centred_and_evenly_spaced() {
        every_layout(|l, _n, _hero| {
            let span = l.board[4].right() - l.board[0].left();
            let mid = (l.board[0].left() + l.board[4].right()) * 0.5;
            assert!((mid - l.centre.x).abs() < 0.5, "the board is not centred");
            assert!(span < l.felt.width() * 0.8, "the board overruns the felt");
            let gaps: Vec<f32> = (0..4)
                .map(|i| l.board[i + 1].left() - l.board[i].right())
                .collect();
            for g in &gaps {
                assert!((g - gaps[0]).abs() < 0.01, "the board is unevenly spaced");
            }
        });
    }

    /// Seats run clockwise on screen from the hero, which is the order of
    /// action. Getting the sign wrong gives a table that looks right and deals
    /// the wrong way, so it is asserted rather than eyeballed.
    #[test]
    fn the_seat_after_the_hero_is_on_the_heros_left_as_drawn() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 700.0));
        for n in 3..=10u8 {
            for hero in 0..n {
                let l = Layout::new(area, n, hero);
                let next = (hero + 1) % n;
                let slot = l.seat(next).unwrap();
                assert!(
                    slot.plate.center().x < l.centre.x,
                    "{n} seats: the seat after the hero should be drawn to the left"
                );
            }
        }
    }

    /// Every seat is on the ring, so the table reads as a table and not as a
    /// scatter of plates.
    #[test]
    fn every_seat_sits_on_the_same_ellipse() {
        every_layout(|l, n, hero| {
            let rx = l.felt.width() * 0.5 + l.metrics.rail * 0.5;
            let ry = l.felt.height() * 0.5 + l.metrics.rail * 0.5;
            for s in &l.seats {
                let d = s.plate.center() - l.centre;
                let on = (d.x / rx).powi(2) + (d.y / ry).powi(2);
                assert!(
                    (on - 1.0).abs() < 0.01,
                    "{n}/{hero}: seat {} is off the ring ({on})",
                    s.seat
                );
            }
        });
    }

    /// The support function is what every offset here is built on, so it is
    /// checked directly: a wide, short box reaches further sideways than up, and
    /// a fixed offset cannot know that.
    #[test]
    fn support_measures_the_direction_it_is_asked_about() {
        let r = Rect::from_center_size(pos2(0.0, 0.0), vec2(100.0, 20.0));
        assert!((support(r, pos2(0.0, 0.0), vec2(1.0, 0.0)) - 50.0).abs() < 0.01);
        assert!((support(r, pos2(0.0, 0.0), vec2(0.0, 1.0)) - 10.0).abs() < 0.01);
    }

    /// And the escape distance is exactly the point where the box comes clear,
    /// not a step past it — the property the bet placement relies on.
    #[test]
    fn escape_is_the_first_distance_that_clears() {
        let zone = vec2(120.0, 40.0);
        let half = vec2(20.0, 8.0);
        let forbidden = Rect::from_center_size(pos2(0.0, 0.0), zone * 2.0);
        for deg in (0..360).step_by(7) {
            let a = (deg as f32).to_radians();
            let dir = vec2(a.cos(), a.sin());
            let t = escape(dir, zone, half);
            let clear =
                Rect::from_center_size(pos2(dir.x * (t + 0.5), dir.y * (t + 0.5)), half * 2.0);
            let inside =
                Rect::from_center_size(pos2(dir.x * (t - 1.0), dir.y * (t - 1.0)), half * 2.0);
            assert!(!clear.intersects(forbidden), "{deg} degrees does not clear");
            assert!(inside.intersects(forbidden), "{deg} degrees cleared early");
        }
    }

    /// A tall window must not produce a circular table, and neither may an
    /// ordinary one. `S1-CS`: the owner called the table round. A poker
    /// table is about twice as wide as it is deep, and the felt follows the
    /// window's width at that proportion rather than filling its height.
    #[test]
    fn a_tall_window_still_gives_a_poker_table() {
        for (w, h) in [(1000.0, 1400.0), (1000.0, 620.0), (1180.0, 640.0), (760.0, 460.0)] {
            let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
            let l = Layout::new(area, 6, 0);
            assert!(
                l.felt.width() / l.felt.height() >= 1.95,
                "at {w}x{h} the felt is {:?}, which is a pond",
                l.felt
            );
        }
    }

    /// The layout scales rather than snapping: doubling the window roughly
    /// doubles the felt, so nothing is pinned to a pixel count.
    #[test]
    fn the_table_scales_with_the_window() {
        let small = Layout::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 520.0)), 6, 0);
        let big = Layout::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1800.0, 1040.0)),
            6,
            0,
        );
        assert!(big.felt.width() > small.felt.width() * 1.6);
        assert!(big.metrics.board_card.x > small.metrics.board_card.x * 1.4);
    }

    /// The squeeze is a safety valve, not the design.
    ///
    /// The proportions above are chosen so an ordinary table needs none of it
    /// and a crowded one needs a little. If this starts failing, the base sizes
    /// have drifted and the cards are being shrunk to cover for them — which is
    /// invisible until the day the valve runs out of travel.
    #[test]
    fn the_middle_of_the_table_never_gives_up_much() {
        every_layout(|l, n, hero| {
            let full = (l.felt.width() * ratio::CARD_W_OF_FELT_W)
                .min(l.felt.height() * ratio::CARD_W_OF_FELT_H);
            let kept = l.metrics.board_card.x / full;
            assert!(
                kept >= 0.85,
                "{n} seats, hero {hero}: the middle shrank to {kept:.2} of its size"
            );
        });
        let comfortable = Layout::new(
            Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 700.0)),
            6,
            0,
        );
        let full = (comfortable.felt.width() * ratio::CARD_W_OF_FELT_W)
            .min(comfortable.felt.height() * ratio::CARD_W_OF_FELT_H);
        assert!(
            (comfortable.metrics.board_card.x - full).abs() < 0.01,
            "an ordinary six-seat table should need no squeeze at all"
        );
    }

    /// A board card is big enough to read at the smallest window the client
    /// allows. The squeeze can only take so much before that stops being true,
    /// and a rank nobody can read is the whole problem this GUI started with.
    #[test]
    fn a_board_card_is_never_too_small_to_read() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(720.0, 390.0));
        for n in 2..=10u8 {
            let l = Layout::new(area, n, 0);
            assert!(
                l.metrics.board_card.x >= 26.0,
                "{n} seats: a board card is only {:.1} across",
                l.metrics.board_card.x
            );
        }
    }

    /// An out-of-range seat count or hero index is clamped rather than
    /// panicking: both arrive from the network in the end.
    #[test]
    fn absurd_inputs_still_produce_a_table() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 600.0));
        assert_eq!(Layout::new(area, 0, 0).seats.len(), 2);
        assert_eq!(Layout::new(area, 200, 199).seats.len(), 10);
        assert_eq!(Layout::new(area, 6, 200).hero().seat, 200 % 6);
    }

    /// A scratch report, not a guard: prints every clash with numbers so the
    /// proportions can be tuned against measurements instead of guesses.
    #[test]
    #[ignore]
    fn report_squeeze() {
        for (w, h) in SIZES {
            let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
            for n in 2..=10u8 {
                let l = Layout::new(area, n, 0);
                let full = (l.felt.width() * ratio::CARD_W_OF_FELT_W)
                    .min(l.felt.height() * ratio::CARD_W_OF_FELT_H);
                println!(
                    "{w}x{h} n={n} scale={:.3} card={:.1}",
                    l.metrics.board_card.x / full,
                    l.metrics.board_card.x
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn report_clashes() {
        for (w, h) in SIZES {
            let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
            for n in 2..=10u8 {
                for hero in 0..n {
                    let l = Layout::new(area, n, hero);
                    let cards = l.all_cards();
                    for s in &l.seats {
                        for c in &cards {
                            if s.bet.intersects(*c) {
                                println!(
                                    "{w}x{h} n={n} hero={hero} seat={} bet={:?} card={:?}",
                                    s.seat, s.bet, c
                                );
                            }
                        }
                        for o in &l.seats {
                            if o.seat != s.seat && s.bet.intersects(o.bet) {
                                println!(
                                    "{w}x{h} n={n} hero={hero} bets {} {} clash",
                                    s.seat, o.seat
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
