# Policy revision 05 regression matrix

No test was run in this policy phase. CLI rows use copied production code in isolated real Git/Cargo repositories and real syscalls. Direct rows invoke the exact production function through private operation objects that scanner/generator `main()` cannot select. A `direct-real` row uses actual FIFO/socket/device/regular filesystem objects or deterministically mutates a real object at a named helper operation. A `CLI smoke` row is unsynchronized and cannot prove a specified syscall interval. Runtime authority remains module-constant `REAL_FILE_OPS`/`REAL_IO_OPS` and immutable limits. An outer test timeout is only a harness guard; special-file boundedness is proved by O_PATH type classification before any data-capable open.

| ID | Level | Stimulus | Required evidence |
| --- | --- | --- | --- |
| BASE-01 | CLI | frozen original scanner/live unchanged tree | exit 1; exact 54, 27/home |
| BASE-02 | CLI+Make | corrected live tree | two homes/56 external Rust; compiler forbid command unchanged |
| ROOT-01 | CLI | relative invocation at root and absolute invocation outside root | same bound root and classification |
| ROOT-02 | CLI | linked/nonregular/malformed invocation name or ancestor | `E_ROOT_BINDING`; zero exemptions |
| ROOT-03 | direct-real | replace final invocation name before second open; touch/replace parent before S1 | exact `E_ROOT_BINDING` at each observed window |
| ROOT-04 | direct+inspection | inspect production helper/main boundary | runtime only `REAL_FILE_OPS`; no loaded-source identity comparison or retained launcher |
| ROOT-05 | CLI smoke | uncontrolled invocation-name replacement | stable success or `E_ROOT_BINDING`; no claim about pre-observation interval |
| TYPE-01 | direct-real | upgrade held `/` O_PATH directory through controlled fd portal | immediate complete identity equality; unsupported fails closed |
| TYPE-02 | direct-real | production helper on real FIFO, Unix socket, `/dev/null` | family `*_NONREGULAR` before data open; recorded portal-data-open count zero |
| TYPE-03 | direct | metadata-open/fstat/data-upgrade/identity/close failures | exact selected-family `*_OPEN` / `*_METADATA` / `*_DATA_OPEN` / `*_RACE` / `*_CLOSE` |
| TYPE-04 | CLI | ordinary ignored/untracked FIFO/socket `.rs` | finite `E_SOURCE_NONREGULAR`; zero data open; zero exemption |
| TYPE-05 | CLI | candidate FIFO/socket/device entry below limits | finite `E_FS_NONREGULAR` + whole-home denial; zero data open |
| TYPE-06 | direct-real | special replacement at final check/archive/output observation | metadata rejection/race before replacement data open |
| TYPE-07 | direct+inspection | main/CLI/env and descriptor portal interface | only REAL ops; portal path uses held decimal fd only; no pathname data fallback |
| TYPE-08 | direct | O_PATH/proc-fd/O_NONBLOCK semantics unavailable | scanner `E_FS_UNSUPPORTED`; generator `E_REGEN_FS_UNSUPPORTED` |
| OWN-01 | CLI | tamper/extra/missing candidate file | exact cache cause; whole-home denial; changed/extra Rust lexical |
| OWN-02 | CLI | ignored/untracked/unattached/cfg-disabled unsafe | `E_UNSAFE` |
| OWN-03 | CLI | unsafe below all six candidate prune-collision names | `E_CACHE_EXTRA` + `E_UNSAFE` below limits |
| OWN-04 | CLI | candidate/repository N/N+1 ceilings | N complete; N+1 zero exemption + incomplete coverage |
| GIT-01 | CLI | outside cwd and all decoy ambient Git/config variables | bound real repository/ownership |
| GIT-02 | CLI | exact binding/config output plus malformed status/LF/NUL | exact accept or binding/config cause |
| GIT-03 | CLI | C0/C1/C2 or T0/T1 changes | `E_GIT_RACE`; zero exemptions |
| GIT-04 | CLI | real index mutation/replacement among I0..I4 or X0/X1 change | `E_GIT_RACE`; zero exemptions |
| GIT-05 | CLI | tracked/staged file, blob ancestor, HEAD/index gitlink | whole-home `E_TRACKED_OVERRIDE` |
| GIT-06 | CLI | real regular default index | all I0..I4 stable/equal and ownership succeeds |
| GIT-07 | direct+real | index OPEN/METADATA/LINK/NONREGULAR/DATA_OPEN/SIZE/READ/SHORT_READ/RACE/CLOSE plus FIFO/socket/device | exact `E_GIT_INDEX_*`; specials never data-opened |
| GIT-08 | direct+CLI | private FileOps inaccessible through main/CLI/env | seam non-authoritative; real CLI remains |
| CARGO-01 | CLI | root+511 versus root+512 package manifests | 512 accepted; 513 Cargo count error below metadata cap |
| CARGO-02 | direct+real | OPEN/METADATA/LINK/NONREGULAR/DATA_OPEN/size/aggregate/read/short/race/close/UTF8/TOML plus FIFO/socket/device | exact Cargo-specific cause; specials never data-opened; never generic FS |
| CARGO-03 | direct-real | replace final name before final reopen; replace/touch parent before M1 | `E_CARGO_MANIFEST_RACE`; parser consumed only original buffer |
| CARGO-04 | CLI | real stable manifests/lint/workspace ownership | type-first data read, metadata-only final name observation, forbid inheritance retained |
| CARGO-05 | CLI smoke | uncontrolled root/member replacement | stable success or exact Cargo failure; no specified-window claim |
| SOURCE-UTF8-01 | CLI | invalid UTF-8 around no unsafe token | all raw bytes charged; U+FFFD text scanned; no UTF-8 error/skip |
| SOURCE-UTF8-02 | CLI | invalid UTF-8 before/after ASCII unsafe | `E_UNSAFE`; exact resulting line; no UTF-8 error |
| CHECK-01 | CLI | exact regular checked manifest | semantic compare succeeds with real syscalls |
| CHECK-02 | CLI | missing/repeated/positional/unknown option; bad grammar/fixed location | respectively `E_REGEN_ARGS`; `E_REGEN_PATH` before filesystem access |
| CHECK-03 | CLI | valid fixed path with parent link/non-directory/mount/escape | `E_REGEN_CONFINEMENT`, never `E_REGEN_CHECK_OPEN` / `E_REGEN_CHECK_LINK` / `E_REGEN_CHECK_RACE` |
| CHECK-04 | CLI+direct-real | initial OPEN/METADATA/LINK/NONREGULAR/DATA_OPEN/size plus FIFO/socket/device | exact `E_REGEN_CHECK_*`; specials never data-opened |
| CHECK-05 | direct | read syscall error; unchanged-tuple early EOF; extra/tuple change | respectively `E_REGEN_CHECK_READ` / `E_REGEN_CHECK_SHORT_READ` / `E_REGEN_CHECK_RACE` |
| CHECK-06 | direct | original or observation close alone/after primary | `E_REGEN_CHECK_CLOSE` primary or retained secondary |
| CHECK-07 | direct | invalid UTF-8/JSON/duplicate/schema | exact UTF8/JSON/SCHEMA cause |
| CHECK-08 | direct-real | replacement between original buffered read and final metadata reopen | `E_REGEN_CHECK_RACE`; original buffer only |
| CHECK-09 | direct-real | replacement after final metadata reopen before D1 | parent-time/identity change; `E_REGEN_CHECK_RACE` |
| CHECK-10 | direct+CLI | inspect private FileOps/main/portal and run real stable CLI | no runtime read bypass/pathname data fallback; real integration retained |
| CHECK-11 | direct | N/N+1 input size | N accepted; N+1 `E_REGEN_CHECK_SIZE` |
| CHECK-12 | static+CLI | cause-set/precedence fixtures | no `E_REGEN_CHECK_PATH`; four disjoint categories exact |
| CHECK-13 | CLI smoke | uncontrolled checked-manifest replacement | stable success or applicable exact failure; no interval claim |
| REGEN-00 | CLI+direct-real | generator relative/absolute invocation; final-name/parent replacement | stable binding or exact `E_REGEN_ROOT_BINDING`; no loaded-code claim |
| REGEN-01 | CLI | exact archive/check/output | deterministic 10-directory/32-file output |
| REGEN-02 | CLI | wrong archive name/size/hash/tamper | identity fails before parser/create |
| REGEN-03 | CLI | bad argument path; parent link/mount; different preexisting parent entry | respectively `E_REGEN_PATH`; `E_REGEN_CONFINEMENT`; `E_REGEN_CONFINEMENT` after fixed-name not-found |
| REGEN-04 | CLI | first final-name check mismatch | `E_REGEN_RACE` + `E_REGEN_RESIDUE` |
| REGEN-05 | direct-real | replacement before second final metadata open | metadata fstat differs from F; race + residue; special not data-opened |
| REGEN-06 | direct-real | replacement immediately after second final metadata open before portal/read | parent P0/P1 change; race + residue; replacement untouched |
| REGEN-07 | direct-real | replacement after descriptor read before P1 | parent change; race + residue; no deletion |
| REGEN-08 | direct-real | replacement in former check-to-unlink window | no unlink exists; replacement untouched; residue retained |
| REGEN-09 | direct | output helper post-create failure | primary cause + residue; zero unlink/rename operations possible |
| REGEN-10 | CLI | stable second final metadata open/portal/read and P0==P1 | success linearizes at metadata-open syscall, not read completion |
| REGEN-11 | CLI smoke | uncontrolled output replacement | success or contract-defined failure; no deletion and no specified-window claim |
| REGEN-12 | CLI | stable fixed output exists as regular/link/FIFO/socket | only `E_REGEN_OUTPUT_EXISTS`; probe precedes parent emptiness; no data open |
| REGEN-13 | CLI | fixed output plus another entry | fixed-name probe wins: only `E_REGEN_OUTPUT_EXISTS` |
| REGEN-14 | direct+CLI | fixed probe failure; create EEXIST; other create failure | respectively `E_REGEN_OUTPUT_PROBE` / `E_REGEN_OUTPUT_EXISTS` / `E_REGEN_OUTPUT_CREATE` |
| ARCHIVE-01 | CLI | official fixed archive through real syscalls | identity before parser/create; stable success |
| ARCHIVE-02 | direct+real | initial OPEN/METADATA/LINK/NONREGULAR/DATA_OPEN/SIZE plus FIFO/socket/device | exact `E_REGEN_ARCHIVE_*`; specials never data-opened |
| ARCHIVE-03 | direct | identity read/unchanged-metadata short read/hash | respectively `E_REGEN_ARCHIVE_IDENTITY_READ` / `E_REGEN_ARCHIVE_IDENTITY_SHORT_READ` / `E_REGEN_ARCHIVE_HASH` |
| ARCHIVE-04 | direct | seek failure/wrong offset | `E_REGEN_ARCHIVE_SEEK`; parser not entered |
| ARCHIVE-05 | direct | parser read syscall/unchanged-metadata short read | `E_REGEN_ARCHIVE_PARSER_READ` / `E_REGEN_ARCHIVE_PARSER_SHORT_READ`, not gzip/tar |
| ARCHIVE-06 | direct-real | post-parser tuple, final-name, or B0/B1 replacement | `E_REGEN_ARCHIVE_RACE` at exact observation |
| ARCHIVE-07 | direct | metadata/data/final-observation close alone or after earlier primary | `E_REGEN_ARCHIVE_CLOSE` primary or retained secondary; primary ordering exact |
| ARCHIVE-08 | direct | compressed and total-decompressed N/N+1 | exact compressed/decompressed limit cause |
| ARCHIVE-09 | direct | header/padding/extensions/end/concatenated/trailing output | every produced byte charged; counter never resets |
| ARCHIVE-10 | direct | payload/count/member/path N/N+1 | exact branch cause |
| ARCHIVE-11 | direct | duplicate/path/link/special/malformed/short/inventory | exact stable parser cause |
| ARCHIVE-12 | CLI smoke | uncontrolled archive-name/parent replacement | stable success or exact confinement/archive race; no interval claim |
| IO-01 | direct | partial/EINTR/zero/error writes | exact success/SHORT_WRITE/WRITE |
| IO-02 | direct | file or parent fsync error | `E_REGEN_FSYNC` + residue after create |
| IO-03 | direct | created/verification close error | `E_REGEN_CLOSE` + residue |
| IO-04 | direct-real | final metadata-open replacement plus parent-time change | `E_REGEN_RACE` + residue |
| IO-05 | direct | inspect REAL_IO_OPS interface | no unlink/unlinkat/rename operation exists |
| IO-06 | CLI | real write/fsync/close/type-first final-observation success | real syscall integration retained |
| IO-10 | direct-real | install replacement in former identity-check-to-unlink window | no deletion primitive/call exists; replacement remains; residue reported |

Every row asserts stable code, exemption scope, raw-byte accounting, and complete/incomplete coverage as applicable. Generic nonzero is insufficient where a cause is named. Direct exact-window tests use the production helper and cannot be activated by scanner/generator runtime. CLI smoke rows are supplemental and cannot satisfy a direct exact-window row.
