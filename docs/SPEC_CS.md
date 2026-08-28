Úkol: vytvoř decentralizovaného pokerového klienta nad libp2p a Mainline DHT
Jsi seniorní software architect, security engineer, kryptografický inženýr a Rust developer se zkušenostmi s P2P protokoly, distribuovanými systémy, libp2p, BitTorrent Mainline DHT, aplikovanou kryptografií, zero-knowledge proofs, MPC/mental poker protokoly a Texas Hold'em pravidly.
Navrhni a postupně implementuj open-source decentralizovaného pokerového klienta, primárně v Rustu, který používá BitTorrent Mainline DHT pouze pro globální discovery hráčů a libp2p pro veškerou další P2P komunikaci, šifrování, NAT traversal a doručování zpráv.
Neexistuje centrální pokerový server, centrální dealer ani centrální databáze, které by během handy znaly všechny karty.
První verze je play-money / technologický prototyp. Neimplementuj v první fázi platby ani skutečné peníze.

1. Základní architektura
Architekturu rozděl minimálně na tyto nezávislé vrstvy:
GUI
│
├── Lobby / matchmaking
│
├── Poker state machine
│
├── Mental Poker / cryptographic deck
│
├── Protocol / signed event log
│
└── P2P transport
│
▼
Mainline DHT (discovery)
libp2p (QUIC, GossipSub, relay)
│
▼
ostatní pokeroví klienti
Síťová vrstva NESMÍ rozhodovat o pokerových pravidlech ani vytvářet karty.
Síťová vrstva má zajišťovat pouze:
bootstrap do Mainline DHT a do libp2p,
globální discovery hráčů pod pevným LOBBY_INFOHASH,
veřejnou pokerovou lobby přes GossipSub,
NAT traversal a hole punching (AutoNAT, DCUtR),
relay fallback přes Circuit Relay v2, aby fungovali i klienti za CGNAT,
šifrovanou a autentizovanou komunikaci mezi peery (Noise nebo TLS 1.3 nad QUIC),
doručování aplikačních pokerových zpráv.
Použij konkrétně tuto kombinaci a nic z ní nenahrazuj vlastní konstrukcí:BitTorrent Mainline DHT pouze pro globální discovery hráčů. Všichni klienti mají stejný pevný LOBBY_INFOHASH a používají announce_peer() a get_peers().libp2p pro veškerou další P2P komunikaci, šifrování, PeerID, QUIC, NAT traversal, hole punching a relay fallback.GossipSub pro globální lobby a pro oznamování TABLE_OPEN, TABLE_UPDATE a TABLE_CLOSE.Nově připojený klient si přes libp2p vyžádá snapshot už existujících stolů od několika peerů a teprve potom pokračuje živou synchronizací přes GossipSub.Heartbeat a TTL, aby offline hráči a zaniklé stoly mizeli sami, bez toho, aby to někdo musel oznámit.Každý stůl má unikátní table_id a jeho zprávy jsou kryptograficky podepsané.Samotný stav pokerové hry se posílá pouze mezi účastníky daného stolu přes přímé libp2p streamy, nikdy přes Mainline DHT.Architektura musí fungovat i pro klienty za NAT a CGNAT a nesmí vyžadovat žádný centrální lobby server.Seznam peerů z Mainline DHT není ověřený: kdokoli tam může zapsat cokoli. Ber ho jako nápovědu, kam se zkusit připojit, nikdy jako tvrzení o tom, kdo tam je. Identitu rozhoduje až libp2p handshake a podpis aplikační zprávy.Pokerová pravidla a zabezpečení balíčku musí být samostatná vrstva.
Použij konkrétně tuto kombinaci a nic z ní nenahrazuj vlastní konstrukcí:
BitTorrent Mainline DHT pouze pro globální discovery hráčů. Všichni klienti mají stejný pevný LOBBY_INFOHASH a používají announce_peer() a get_peers().
libp2p pro veškerou další P2P komunikaci, šifrování, PeerID, QUIC, NAT traversal, hole punching a relay fallback.
GossipSub pro globální lobby a pro oznamování TABLE_OPEN, TABLE_UPDATE a TABLE_CLOSE.
Nově připojený klient si přes libp2p vyžádá snapshot už existujících stolů od několika peerů a teprve potom pokračuje živou synchronizací přes GossipSub.
Heartbeat a TTL, aby offline hráči a zaniklé stoly mizeli sami, bez toho, aby to někdo musel oznámit.
Každý stůl má unikátní table_id a jeho zprávy jsou kryptograficky podepsané.
Samotný stav pokerové hry se posílá pouze mezi účastníky daného stolu přes přímé libp2p streamy, nikdy přes Mainline DHT.
Architektura musí fungovat i pro klienty za NAT a CGNAT a nesmí vyžadovat žádný centrální lobby server.
Seznam peerů z Mainline DHT není ověřený: kdokoli tam může zapsat cokoli. Ber ho jako nápovědu, kam se zkusit připojit, nikdy jako tvrzení o tom, kdo tam je. Identitu rozhoduje až libp2p handshake a podpis aplikační zprávy.
Pokerová pravidla a zabezpečení balíčku musí být samostatná vrstva.

2. Nejdříve ověř skutečná API libp2p a Mainline DHT
Před psaním síťové vrstvy prostuduj aktuální dokumentaci a zdrojový kód rust-libp2p.
Použij oficiální crate libp2p a ověř ve verzi, kterou připneš, které features skutečně obsahuje: quic, noise, tls, gossipsub, kad, autonat, dcutr, relay, identify a ping.
Pro Mainline DHT ověř, která crate umí announce_peer() a get_peers() jako klient, ne jen jako crawler, a jestli je udržovaná. Pokud žádná vhodná není, napiš minimálního KRPC klienta sám: je to bencode nad UDP a je to malý, dobře popsaný protokol. Nepoužívej k tomu celou torrentovou knihovnu.

Nikdy nevymýšlej názvy typů, funkcí ani cargo features, které crate ve skutečnosti nemá.
Pokud stack některou potřebnou část neumí:
zdokumentuj přesně, která funkce chybí,
ověř to ve zdrojovém kódu crate a ve specifikaci libp2p,
napiš minimální doplněk nad existující crate, ne vlastní protokol,
případně přispěj upstream a mezitím si drž vlastní patch,
Rust musí zůstat jediným jazykem aplikační vrstvy.
Nepiš vlastní P2P protokol, vlastní šifrování ani vlastní NAT traversal.

3. Jedna společná decentralizovaná Poker Lobby
Každá instalace aplikace musí obsahovat stejné předem známé:
LOBBY_INFOHASH
Toto je pevný 20bajtový infohash v BitTorrent Mainline DHT. Pod ním se klienti navzájem najdou, aniž by kdokoli musel znát kohokoli předem.
Po spuštění aplikace má klient automaticky:
start
 ↓
load persistent libp2p keypair
↓
bootstrap Mainline DHT
↓
announce_peer(LOBBY_INFOHASH)
↓
get_peers(LOBBY_INFOHASH)
↓
dial peers over libp2p
↓
subscribe GossipSub lobby topic
↓
request table snapshot from several peers
↓
receive available tables
Uživatel nemusí znát PeerId ani adresu ostatních hráčů a nemusí si s nikým nic vyměnit předem. Předpokládej, že lidé, kteří si chtějí zahrát, na sebe žádný kontakt nemají.
Klient nesmí při každém spuštění vytvářet novou lobby.
LOBBY_INFOHASH bude konfigurační konstanta distribuovaná se všemi kompatibilními klienty.
Správně vyřeš persistentní identitu: Ed25519 keypair, ze kterého vzniká PeerId, ulož bezpečně a použij ho znovu při dalším spuštění. Dvě instalace nesmí sdílet jeden PeerId.
Zdokumentuj také omezení tohoto řešení: záznamy v Mainline DHT nejsou nijem ověřené a mají krátkou životnost, announce se musí opakovat, a veřejný infohash je průběžně sledovaný, takže announce zveřejňuje IP adresu hráče jako někoho, koho zajímá tato jedna konkrétní hodnota. Nic z toho není důvod k centrálnímu serveru, ale hráč to má vědět.
Nevytvářej skrytý centrální fallback server.

4. Decentralizovaný matchmaking
V lobby mohou klienti zveřejňovat podepsané nabídky stolů například:
TABLE_ADVERTISEMENT

protocol_version
table_id
game = NLHE
mode = CASH_PLAY_MONEY
small_blind
big_blind
min_buyin
max_buyin
players
max_players
table_public_key
timestamp
expires_at
signature
Lobby nesmí být autoritou nad hrou.
Po nalezení stolu si účastníci vytvoří samostatnou table session.
Lobby pak slouží jen k discovery.
Implementuj expiraci starých inzerátů a ochranu proti spamování.
Turnaj má předdefinované parametry po vzoru hodnocené hry Sit and Go v PokerTH, takže hráč nemusí nic nastavovat: pevný počet míst, stejný startovní stack pro všechny, blindy podle pevného rozpisu, které se zvyšují po daném počtu hand, a časový limit na akci a na handu. Konkrétní hodnoty odvoď z výchozího hodnoceného Sit and Go v PokerTH a zdokumentuj je jako jeden pojmenovaný preset.
Zakladatel stolu v lobby smí místo presetu použít vlastní parametry: počet míst, startovní stack, rozpis blindů, rychlost zvyšování, časové limity a případně heslo ke stolu. Vlastní parametry musí být součástí podepsaného inzerátu stolu, aby každý, kdo se připojuje, viděl přesně, do čeho jde, a aby se na nich všichni účastníci shodli ještě před rozdáním první handy.
Jakmile handa skončí, ať už showdownem, nebo tím, že všichni soupeři složili, musí se automaticky rozdat další handa. Turnaj pokračuje bez ručního zásahu, dokud nezbude jeden hráč se všemi žetony nebo dokud se stůl nerozpadne. Přechod na další handu je součástí state machine a musí být pro všechny účastníky deterministický a ověřitelný, ne řízený tím, kdo první klikne.

5. Žádný důvěryhodný dealer
Nejdůležitější bezpečnostní požadavek:
Nikdy nesmí existovat jeden hráč, server nebo proces, který před rozdáním zná celý zamíchaný balíček, hole cards všech hráčů a budoucí board.
Nepoužívej model:
host vytvoří deck
host zamíchá
host rozdá ostatním
To je nepřijatelné.
Použij skutečný Mental Poker protocol.

6. Neimplementuj vlastní kryptografii od nuly
Nejdříve proveď technický průzkum několika akademicky známých mental-poker/MPC konstrukcí.
Vyber protokol, který poskytuje alespoň:
distribuované vytvoření náhodného balíčku,
kryptograficky bezpečnou náhodnost,
verifiable shuffle,
re-encryption nebo ekvivalent,
důkaz, že při shuffle nebyla karta přidána, odstraněna nebo změněna,
soukromé rozdání hole cards,
selektivní odhalování karet,
nemožnost předčasně zjistit board,
ochranu proti jednomu modifikovanému/malicious klientovi,
kryptograficky ověřitelný transcript.
Preferuj zavedené a auditované kryptografické knihovny.
Nevytvářej vlastní šifru, hash, RNG, ZK proof systém ani vlastní elliptic-curve konstrukci, pokud existuje zavedená knihovna.
Preferuj auditované Rust crates (například curve25519-dalek, ed25519-dalek, k256, sha2, rand). Pokud pro potřebný protokol žádná vhodná Rust knihovna neexistuje, zdokumentuj to a navrhni řešení, ale nepiš si vlastní kryptografickou konstrukci.

7. Bezpečné vytvoření náhodnosti
Jeden hráč nesmí rozhodovat RNG celé handy.
Použij kryptograficky bezpečnou distribuovanou náhodnost, například vhodný commit/reveal nebo mechanismus požadovaný zvoleným mental-poker protokolem.
Koncept:
A: commit(random_A)
B: commit(random_B)
C: commit(random_C)

↓

všichni zveřejní potřebné hodnoty

↓

combined randomness
Jeden hráč nesmí být schopen po zjištění náhodnosti ostatních změnit svůj původní příspěvek.
Použij OS CSPRNG.
Nikdy nepoužívej nekryptografický generátor pro kryptografické účely: používej OsRng, ne SmallRng ani generátor se vlastním seedem.

8. Verifiable shuffle
Každý shuffle musí být kryptograficky ověřitelný.
Musí být možné ověřit tvrzení:
output deck
=
stejné karty jako input deck
+
tajná permutace
+
povolená kryptografická transformace
bez zveřejnění tajné permutace.
Pokud proof nesedí:
INVALID_SHUFFLE_PROOF
hand se nesmí dál hrát.
Peer musí být označen jako zdroj protokolového selhání.
Podvod typu:
odstraním 2c
přidám druhé As
musí být kryptograficky detekovatelný.

9. Soukromé hole cards
Hole cards hráče A smí být dešifrovatelné pouze hráčem A, dokud pravidla hry nevyžadují jejich zveřejnění.
Příklad požadovaného bezpečnostního stavu:
Alice PC:

Alice = Ah Kh
Bob   = encrypted/unreadable
Carol = encrypted/unreadable
Bob PC:

Alice = encrypted/unreadable
Bob   = Qs Qd
Carol = encrypted/unreadable
Pouhé:
card.visible = False
NENÍ zabezpečení.
Cizí hole card nesmí být v paměti klienta v plaintextu a pouze skrytá GUI.
Ani modifikovaný klient nesmí být schopen zobrazit cizí hole cards, protože nemá kryptografický materiál potřebný k jejich dešifrování.

10. Budoucí board nesmí být předem znám
Před flopem nesmí žádný jednotlivý hráč vědět:
flop
turn
river
Karty boardu se mají kryptograficky zpřístupnit až při dosažení příslušné street.
Například:
PRE-FLOP

flop  = encrypted
turn  = encrypted
river = encrypted
a až poté:
FLOP_REVEAL
      ↓
společná kryptografická operace
      ↓
Qs 8c 2d
Stejný princip pro turn a river.

11. Deterministický poker engine
Implementuj pravidla No-Limit Texas Hold'em jako čistou deterministickou state machine.
Stejné vstupy musí na všech klientech vytvořit identický stav.
State musí zahrnovat minimálně:
table_id
hand_id
button
small blind
big blind
player order
stack
current bet
pot
side pots
street
player_to_act
minimum_raise
last_full_raise
all-in states
fold states
board
betting history
Správně implementuj:
blinds,
heads-up blind/button pravidla,
check,
call,
bet,
raise,
min-raise,
all-in,
incomplete all-in raise,
reopening betting,
fold,
side pots,
split pots,
ties,
showdown,
button rotation.
Poker engine nesmí důvěřovat tomu, že protistrana posílá legální akce.
Každou příchozí akci znovu lokálně ověř.

12. Podepisované protokolové události
Vytvoř samostatný aplikační signing keypair, například Ed25519 pomocí zavedené knihovny.
Každá kritická událost musí obsahovat například:
protocol_version
table_id
hand_id
sequence
sender_public_key
event_type
payload
previous_event_hash
timestamp/deadline information
signature
Použij kanonickou serializaci.
Například deterministic CBOR nebo jiný skutečně deterministický formát.
Podepisuj přesně definované bajty.
Nepodepisuj nekanonický JSON.

13. Hash-chain transcript handy
Každá handa má vytvořit kryptografický transcript:
GENESIS
  ↓ hash
HAND_INIT
  ↓ hash
SHUFFLE_1
  ↓ hash
SHUFFLE_2
  ↓ hash
DEAL
  ↓ hash
RAISE
  ↓ hash
CALL
  ↓ hash
FLOP_REVEAL
  ↓
...
Všichni hráči musí být schopni ověřit celý transcript.
Transcript nesmí obsahovat tajné kryptografické hodnoty, které by umožnily rekonstruovat cizí karty.
Po dokončení handy má být možné ověřit:
kdo provedl jednotlivé akce,
pořadí akcí,
že nikdo nezměnil historii,
že shuffle proofs byly validní,
že zveřejněné board cards odpovídají původnímu deck commitmentu,
že vítěz a velikost potu byly vypočteny správně.

14. Ochrana proti replay a equivocation
Implementuj ochranu proti:
replay attack
duplicate message
out-of-order message
message from another hand
message from another table
old signed message
equivocation
Používej:
table_id
hand_id
session nonce
monotonic sequence
previous_hash
signature
Pokud hráč pošle různým peerům dvě konfliktní podepsané události pro stejnou sekvenci, musí vzniknout kryptografický důkaz jeho equivocation.

15. Synchronizace stavu
Po kritických přechodech mohou klienti potvrzovat:
STATE_HASH
Například:
state_hash =
HASH(canonical_serialized_public_state)
Pokud:
Alice state = A7F3...
Bob state   = A7F3...
Carol state = 91BC...
hra se nesmí tiše pokračovat.
Spusť synchronizační/dispute mechanismus.
Neřeš konflikt pravidlem „host má vždy pravdu“.

16. Navrhni aplikační protokol
Navrhni verzovaný protokol například s událostmi:
HELLO
CAPABILITIES
LOBBY_TABLE_AD
LOBBY_TABLE_REMOVE

JOIN_REQUEST
JOIN_ACCEPT
PLAYER_LIST
TABLE_READY

HAND_INIT

RNG_COMMIT
RNG_REVEAL

DECK_INIT
SHUFFLE_STEP
SHUFFLE_PROOF
DECK_COMMIT

DEAL_PRIVATE

ACTION_CHECK
ACTION_CALL
ACTION_BET
ACTION_RAISE
ACTION_FOLD

BOARD_REVEAL
SHOWDOWN_REVEAL

STATE_HASH
STATE_ACK
DISPUTE

HAND_COMPLETE

PLAYER_LEAVE
TIMEOUT
Používej explicitní schema validation.
Nepoužívej pickle.
Nikdy nepoužívej eval() nad daty ze sítě.

17. Chování malicious klienta
Threat model musí předpokládat, že protivník má kompletně modifikovaného klienta.
Nepředpokládej:
„GUI mu nedovolí kliknout na tuto možnost.“
Předpokládej, že útočník může libovolně vytvářet síťové pakety.
Protokol musí detekovat nebo kryptograficky znemožnit zejména:
falešnou kartu,
duplicitní kartu,
odstranění karty z decku,
manipulaci shuffle,
manipulaci RNG,
čtení cizích hole cards,
předčasné čtení boardu,
nelegální pokerovou akci,
falešnou velikost stacku,
falešný pot,
akci mimo pořadí,
změnu již podepsané akce,
replay starých akcí,
změnu historie handy,
vydávání se za jiného účastníka,
různé verze historie zasílané různým hráčům,
malformed packets,
oversized packets,
resource exhaustion v rozumné míře.

18. Co kryptografie neumí úplně vyřešit
Neslibuj absolutní „nemožnost jakéhokoli podvádění“.
Explicitně rozlišuj mezi:
protocol cheating
a
real-world / endpoint cheating
Protokol má maximálně zabránit nebo detekovat podvody proveditelné prostřednictvím samotného pokerového protokolu.
Samotná kryptografie nedokáže úplně odstranit například:
koluzi dvou hráčů, kteří si mimo aplikaci sdělí hole cards,
malware běžící na počítači hráče a čtoucí jeho vlastní karty,
screen sharing,
multi-account/Sybil útoky bez samostatné identity/reputation vrstvy,
fyzické donucení,
traffic analysis,
denial-of-service,
úmyslné vypnutí klienta.
Tyto limity jasně dokumentuj.

19. Disconnect / abort attack
Mental poker protokoly mají problém s hráčem, který přestane spolupracovat ve vhodném okamžiku.
Vyřeš to explicitně.
Nejprve zjisti, zda zvolený kryptografický protokol obsahuje robustní mechanismus pro dokončení handy po odpojení.
Pokud ne, MVP musí mít přesně definované:
timeout
hand abort
evidence of which peer failed
reputation penalty
Nikdy nevytvářej mechanismus, který kvůli odolnosti proti disconnectu umožní několika hráčům předčasně dešifrovat hole cards ostatních.
Security má přednost před pohodlným dokončením handy.

20. Oddělení identity
Rozlišuj minimálně:
libp2p PeerId
Poker application identity
Table/session identity
Dlouhodobou pokerovou identitu podepisuj vlastním aplikačním klíčem.
Pro každou table session používej unikátní nonce/session ID.
Nikdy nepovažuj PeerId ani samotné libp2p spojení za autentizaci aplikačních dat. PeerId říká, s jakým socketem mluvíš, ne kdo hraje.

21. Lokální ochrana klíčů
Persistentní secret keys ukládej bezpečně.
Na Windows pokud možno využij OS mechanismus typu DPAPI/credential storage.
Na Linuxu podporuj bezpečný keyring nebo soubor s přísnými permissions.
Do logů nikdy nezapisuj:
private keys
private hole cards ostatních hráčů
decryption shares, pokud musí zůstat tajné
raw RNG secrets před jejich bezpečným zveřejněním
Citlivé buffery pokud možno po použití vymaž.

22. GUI a vzhled
Pro Rust preferuj například egui/eframe nebo Slint, pokud nenajdeš lepší důvod pro jinou technologii. Rozhodující je, že z toho vyjde jeden spustitelný soubor bez runtime, který se dá jen zkopírovat.
Klient musí být portable: jediný přenositelný adresář, žádný instalátor, žádný zápis do registru ani mimo vlastní složku, a profil (identita, nastavení, historie) uložený vedle programu, aby šel celý adresář zkopírovat na jiný stroj.
Lobby udělej po vzoru PokerTH: seznam dostupných stolů se sloupci hra, blindy, obsazení a stav, seznam hráčů, chat a tlačítko pro připojení, plus vytvoření vlastního stolu.
Pokerový stůl dej do samostatného okna a vizuálně vyjdi ze souboru ggpoker-rush-and-cash-table.jpg, který je přiložený k tomuto zadání: ovál stolu, barvy, rozložení sedadel kolem něj, pot ve středu, board nad potem, jména a stacky u sedadel. Karty udělej také podle toho vzoru, včetně rubu a stylu hodnot a barev.

GUI minimálně:
Main Window
│
├── Network status
│   ├── libp2p connected
│   ├── DHT status
│   └── Lobby status
│
├── Lobby
│   ├── available tables
│   ├── players
│   ├── game
│   ├── blinds
│   └── join
│
└── Poker Table
    ├── seats
    ├── stacks
    ├── board
    ├── pot
    ├── hole cards
    ├── action buttons (fold, check, call, bet, raise, all-in)
    ├── bet slider
    ├── bet amount field
    ├── quick bets (1/2 pot, pot, all-in)
    ├── timer
    └── protocol/security status
Nikdy nezobrazuj kryptograficky neověřenou kartu jako platnou.

23. Architektura zdrojového kódu
Navrhni projekt přibližně:
p2p-poker/
│
├── Cargo.toml
├── Cargo.lock
│
├── src/
│ ├── main.rs
│ ├── app/
│ ├── gui/
│ │ ├── lobby.rs
│ │ ├── table.rs
│ │ └── theme.rs
│ │
│ ├── net/
│ │ ├── dht.rs
│ │ ├── swarm.rs
│ │ ├── lobby.rs
│ │ └── streams.rs
│ │
│ ├── protocol/
│ │ ├── messages.rs
│ │ ├── serialization.rs
│ │ ├── signatures.rs
│ │ └── transcript.rs
│ │
│ ├── poker/
│ │ ├── state.rs
│ │ ├── engine.rs
│ │ ├── actions.rs
│ │ ├── pots.rs
│ │ ├── tournament.rs
│ │ └── evaluator.rs
│ │
│ ├── mental_poker/
│ │ ├── protocol.rs
│ │ ├── deck.rs
│ │ ├── shuffle.rs
│ │ ├── proofs.rs
│ │ └── reveal.rs
│ │
│ ├── security/
│ │ ├── keys.rs
│ │ ├── rng.rs
│ │ ├── validation.rs
│ │ └── limits.rs
│ │
│ └── storage/
│
├── tests/
│ ├── protocol/
│ ├── adversarial/
│ └── fuzz/
│
├── assets/
│ ├── table/
│ └── cards/
│
└── docs/
Strukturu uprav podle skutečných potřeb, ale zachovej oddělení transportu, poker engine a cryptographic decku.

24. Testovací režim bez sítě
Pokerový engine a mental-poker protokol musí jít testovat bez živé sítě, bez DHT a bez libp2p.
Vytvoř:
InMemoryTransport
který simuluje peer-to-peer síť.
Musí podporovat testování:
delay,
duplicate,
packet loss, pokud relevantní,
reordered events,
disconnect,
reconnect,
malicious packets,
conflicting messages.
Díky tomu musí jít tisíce hand automaticky testovat bez DHT a bez sítě.

25. Adversarial test suite
Vytvoř speciální malicious peer implementace.
Například:
CheaterDuplicateAce
CheaterReplaceCard
CheaterInvalidShuffle
CheaterPredictableRNG
CheaterReplayAction
CheaterIllegalRaise
CheaterFakeStack
CheaterEquivocation
CheaterReadOpponentCard
CheaterFutureBoard
CheaterDisconnect
Každý test musí dokazovat, že:
podvod je kryptograficky nemožný
nebo:
podvod je jednoznačně detekován a handa zastavena
podle threat modelu.
Zvlášť vytvoř test:
Modifikovaný klient nesmí být schopen získat plaintext soupeřových hole cards z dat, která legitimně obdržel přes síť.
A test:
Jeden malicious hráč nesmí být schopen zvolit budoucí board manipulací posledního RNG/shuffle kroku.

26. Property-based testing
Použij proptest nebo quickcheck.
Kontroluj invarianty například:
celkový počet chipů se nemění
žádná karta se nevyskytuje dvakrát
board má maximálně 5 karet
hráč nemůže vsadit více než stack
pot = součet odpovídajících příspěvků
folded player nemůže vyhrát pot
side pot obsahuje pouze oprávněné hráče
state machine nemůže přeskočit street
neplatný podpis nikdy nezmění state

27. Fuzzing
Fuzzuj parser všech dat přijatých ze sítě.
Parser nesmí při libovolném vstupu:
crashnout
alokovat neomezenou paměť
spustit kód
číst mimo buffer
obejít schema validation
Zaveď maximální velikosti všech zpráv a kolekcí.

28. Dependency security
Používej minimální počet dependencies.
Pro každou security-critical dependency zaznamenej:
název
verzi
účel
repozitář
licenci
security status
Pinuj verze reproducibilním způsobem: commitni Cargo.lock a kontroluj závislosti pomocí cargo-deny a cargo-audit.
Nevkládej náhodný neudržovaný kryptografický GitHub projekt jen proto, že obsahuje slovo „mental poker“.
Kryptografická vhodnost má přednost před rychlostí implementace.

29. Dokumentace protokolu před implementací
Než implementuješ mental-poker vrstvu, vytvoř:
docs/THREAT_MODEL.md
docs/PROTOCOL.md
docs/CRYPTOGRAPHY.md
docs/NETWORK_STACK.md
docs/STATE_MACHINE.md
THREAT_MODEL.md musí přesně definovat:
čemu důvěřujeme,
čemu nedůvěřujeme,
schopnosti útočníka,
bezpečnostní cíle,
známá omezení.
CRYPTOGRAPHY.md musí přesně uvést, který existující mental-poker protokol je použit a z jakých odborných zdrojů vychází.
Nevytvářej bezpečnostní konstrukci pouze na základě intuice.

30. Postup implementace
Pracuj po kontrolovatelných etapách:
Phase 0
research libp2p + Mainline DHT + mental poker protocols

Phase 1
threat model + protocol specification

Phase 2
deterministic poker engine

Phase 3
in-memory P2P protocol

Phase 4
signatures + transcript + validation

Phase 5
mental-poker cryptography

Phase 6
adversarial tests

Phase 7
libp2p transport

Phase 8
Mainline DHT discovery + GossipSub lobby

Phase 9
GUI

Phase 10
full multi-peer integration tests

Phase 11
security audit
Po každé fázi spusť testy.
Nepokračuj přes chybu pouze proto, aby aplikace „nějak běžela“.

31. Git
Používej Git od začátku.
Vytvářej malé logické commity.
Nikdy nepřepisuj nebo nemaž velké části funkční implementace bez zdůvodnění.
Před security-critical změnou přidej regresní test, který reprodukuje problém.

32. První podporovaný režim
Nejdříve implementuj:
2-player Heads-Up
No-Limit Texas Hold'em
Play Money
Až když bude protokol bezpečně fungovat pro dva hráče, rozšiř jej na:
3–6 players
side pots
multiple all-ins
disconnect handling
Kryptografický návrh ale od začátku navrhni tak, aby multi-player rozšíření nebylo nemožné.

33. Výkon
Kryptograficky náročné operace nesmí blokovat GUI event loop.
Použij worker threads/processes nebo nativní async mechanismus podle konkrétní knihovny.
Profiluj:
shuffle generation
shuffle verification
private deal
board reveal
signature verification
network latency
hand startup latency
Neoptimalizuj odstraněním bezpečnostních kontrol.

34. Výsledek projektu
Cílová uživatelská zkušenost:
uživatel spustí aplikaci
        ↓
klient se ohlásí v Mainline DHT
a připojí se přes libp2p
        ↓
klient automaticky vstoupí
do známé Poker Lobby
        ↓
vidí dostupné stoly
        ↓
vybere stůl
        ↓
naváže P2P spojení
        ↓
všichni hráči společně
kryptograficky vytvoří deck
        ↓
začne poker
Bez:
centrálního poker serveru
centrálního dealera
centrální databáze hand
admina, který vidí všechny hole cards
serveru, který zná budoucí board

35. Hlavní bezpečnostní invariant
Během každé handy musí pokud možno platit:
Žádný jednotlivý účastník nesmí být schopen zjistit cizí neodhalené hole cards ani budoucí board a žádný jednotlivý účastník nesmí být schopen změnit složení nebo pořadí balíčku bez kryptograficky detekovatelného porušení protokolu.
A současně:
Každý klient musí být schopen nezávisle ověřit všechny veřejné pokerové state transitions a kryptografické důkazy potřebné pro potvrzení korektnosti handy.
Nespoléhej na důvěru v oficiální klient.
Bezpečnost musí zůstat zachována i proti modifikovanému malicious klientovi, v mezích explicitně definovaného threat modelu.

36. Důležité pravidlo
Pokud narazíš na část, u které si nejsi kryptograficky jistý, nezjednodušuj ji vlastní konstrukcí.
Místo toho:
zastav implementaci této části,
popiš konkrétní problém,
najdi zavedený protokol nebo peer-reviewed řešení,
porovnej jeho vlastnosti s naším threat modelem,
teprve potom jej implementuj.
Security-critical tvrzení musí být doložitelné.

Začni nyní
Nezačínej okamžitě generováním tisíců řádků kódu.
Nejdříve proveď Phase 0:
ověř aktuální rust-libp2p a dostupné knihovny pro Mainline DHT,
ověř announce_peer() a get_peers() nad pevným LOBBY_INFOHASH, GossipSub, DCUtR a Circuit Relay v2, a změř, jestli se dva klienti na různých sítích skutečně najdou a spojí,
najdi několik vhodných moderních mental-poker/verifiable-shuffle protokolů,
porovnej jejich security properties, licence, implementační náročnost a dostupné knihovny,
doporuč konkrétní variantu,
vytvoř THREAT_MODEL.md a návrh PROTOCOL.md,
identifikuj všechny body, kde nelze garantovat ochranu proti podvádění.
Teprve potom začni implementovat kostru projektu a automatické testy.
Nikdy netvrď, že systém „znemožňuje veškeré podvádění“. Přesně dokazuj, které třídy útoků kryptograficky znemožňuje, které pouze detekuje a které jsou mimo možnosti protokolu.
