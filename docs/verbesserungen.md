# 100 Verbesserungsvorschläge

Aus der Sicht eines Nutzers, nicht eines Entwicklers. **Nichts davon ist umgesetzt** —
die Liste geht zuerst an Ben.

Jeder Punkt nennt **was**, **warum** (aus der Handhabung heraus) und **wo**. Kritik ist
Absicht: „zu viel", „überflüssig", „gehört weg" stehen gleichberechtigt neben neuen
Funktionen. Innerhalb einer Gruppe stehen die aus meiner Sicht nützlichsten oben.

---

## A · Was beim ersten Start passiert

1. **Ein leeres Raster erklärt sich nicht.** Wer das Programm zum ersten Mal öffnet,
   sieht Kacheln aus Nichts und eine Eingabezeile. Eine einzige Zeile in der Mitte
   („Schreib unten etwas und drücke Speichern") würde reichen. *Wo:* Raster, leerer
   Zustand.
2. **Es gibt keinen Beispiel-Prompt.** Drei mitgelieferte Beispiele — einer mit
   Variable, einer mit Bild, einer mit Markdown — zeigen in zehn Sekunden, was das
   Programm kann, und sind mit einem Klick weg. *Wo:* erster Start.
5. **Die Standardansicht heißt wie ein Platzhalter.** Ein Name wie „Meine Prompts"
   trägt mehr als „Ansicht 1". *Wo:* erste Ansicht.

## B · Raster und Kacheln

6. **Eine Kachel zeigt nicht, wie lang ihr Prompt ist.** Zwei Zeilen oder zwei Seiten
   sehen identisch aus. Ein sehr dezenter Balken oder eine Zeichenzahl im Tooltip
   würde die Auswahl beschleunigen. *Wo:* Kachel. - Mache nur zeichen und token zahl im tooltip
7. **Kopieren gibt zu wenig zurück.** Die Kachel blinkt kurz. Bei einem langen Prompt
   wüsste man gern, dass wirklich alles kopiert wurde — „1.240 Zeichen kopiert" im
   Toast. *Wo:* Kopier-Feedback.
8. **Es fehlt „an dieselbe Stelle zurück".** Verschiebt man eine Kachel versehentlich,
   gibt es kein Rückgängig. Ein einmaliges Undo direkt nach dem Ablegen. *Wo:* Raster. - mache dafür temporären undo button der nach paar sekunden wieder verschwindet mit schönem icon und schön animiert
9. **Leere Zellen sind totes Gebiet.** Ein Doppelklick auf eine leere Zelle könnte
   direkt dort einen neuen Prompt anlegen, statt unten zu schreiben und dann zu
   ziehen. *Wo:* Raster.
10. **Die Rasterspalten-Steuerung oben ist ein Nischenwerkzeug.** Sie steht dauerhaft
    in der Kopfzeile und wird selten benutzt. Sie gehört in die Ansichts-Einstellungen
    — oder bleibt, aber nur solange man mit der Maus in der Kopfzeile ist. *Wo:*
    Kopfzeile (Schnellraster). - ja blende das nur ein wenn maus da ist. mache das aber als option im expertenmenü als standard aus
11. **Mehrfachauswahl gibt es nur in der Bibliothek.** Im Raster wäre sie genauso
    nützlich (mehrere Kacheln färben, verschieben, löschen). *Wo:* Raster. - mache dafür wenn man über button hovert oben links eine checkbox erscheinen, wenn man die auswählt erscheinen alle checkboxen aller buttons, man kann dann alle auswählen, oben in werkzeugleiste soll dann optionen sein für färben, verschieben, etc.
12. **Kein „Prompt duplizieren".** Varianten eines Prompts baut man ständig; heute
    heißt das kopieren, neu anlegen, einfügen. *Wo:* Kachel-Kontextmenü.
14. **Kein Hinweis, wenn ein Prompt eine Variable enthält.** Man merkt es erst beim
    Kopieren, wenn ein Dialog aufgeht. Ein kleines Zeichen auf der Kachel. *Wo:* Kachel. - färbe variablen im prompt text ein

## C · Bibliothek

17. **Es fehlt eine Sortierung „zuletzt bearbeitet".** Man sucht öfter, was man
    kürzlich geändert hat, als was man kürzlich kopiert hat. *Wo:* Bibliothek → Sortierung.
18. **Die Suche kann nur suchen, nicht ausschließen.** Ein `-wort` würde bei vielen
    ähnlichen Prompts sofort helfen. *Wo:* Bibliothek → Suche.
19. **Kein Zähler in der Kopfzeile.** „37 von 214" sagt sofort, wie stark ein Filter
    greift. *Wo:* Bibliothek.
20. **Die Zeilenhöhe ist für lange Prompts zu knapp.** Zwei Zeilen Vorschau statt
    einer, umschaltbar, würde beim Wiedererkennen mehr helfen als jedes Abzeichen.
    *Wo:* Bibliothekszeile.
21. **Löschen fragt jedes Mal.** Bei einer Aufräumaktion nervt das. Eine Rückfrage pro
    Sitzung, danach Undo im Toast. *Wo:* Bibliothek → Löschen. - Nein mache einfach änderung des icons, färbe button erst komplett rot, dann nochmal drücken zum löschen, dann toast(expertenmenü optionen!)
22. **Die Spaltenanzahl der Bibliothek ist eine Einstellung zu viel.** Sie steht in
    der Kopfzeile, im Expertenmenü und wirkt nur in der Bibliothek. Eine Stelle reicht.
    *Wo:* Bibliothek. - Entferne spalten toggler in bibliothek!

## D · Tags

23. **Tags sind das am besten versteckte Feature des Programms.** Sie sind
    standardmäßig aus, die Filterleiste steckt in der Bibliothek und heißt nichts.
    Vorschlag: Tags an ausliefern. *Wo:* Expertenmenü, Bibliothek.
24. **Kein Tag-Filter im Raster.** Getaggt wird im Editor, gefiltert nur in der
    Bibliothek — genau dort sucht aber niemand, der sein Raster vor Augen hat.
    *Wo:* Raster.
25. **Es gibt keine Tag-Vorschläge beim Anlegen eines Prompts.** Der Chevron am Feld
    hilft, aber die drei häufigsten Tags direkt als Chips anzubieten wäre schneller.
    *Wo:* Editor.
26. **Der Tag-Manager ist gut, aber schwer zu finden.** Er hängt am Ende der
    Filterleiste als grauer Knopf. *Wo:* Bibliothek.
27. **Tag-Farben lassen sich nicht auf einmal setzen.** Wer zehn Tags anlegt, klickt
    zwanzig Mal. Eine Palette im Manager, die reihum vergibt. *Wo:* Tag-Manager.

## E · Editor

28. **Der Editor ist voll.** Name, Favorit, Medien, Farbe, Tags, Notizen, Variablen,
    Hotkey, Schrift, Größe, Verlauf — alles gleichzeitig.
    *Wo:* Editor. - Blende nicht oft genutztes wie zb hotkey, versionsverlauf, tags unter einem einklapp menü erweiterte optionen ein.
29. **Die Zeichen-/Token-Anzeige interessiert die meisten nicht.** Sie ist eine
    Zahl ohne Handlung. Als Tooltip auf der Kachel genügt sie. *Wo:* Editor.
31. **Die Versionsgeschichte zeigt keinen Unterschied.** Man sieht alte Fassungen,
    aber nicht, was sich geändert hat. Ohne Diff ist der Wert klein. *Wo:* Editor → Verlauf.
32. **Das Schrift- und Größenpaar pro Kachel ist Überbau.** Es betrifft nur die Optik
    einer einzelnen Kachel und steht mitten im Editor. Gehört ins Kontextmenü der
    Kachel. *Wo:* Editor. - lösche nicht sondern verschiebe nach oben direkt unter zeile mit titel eingabe! so passt das besser zusammen
33. **Der Variablen-Helfer erklärt die Syntax, aber fügt nicht ein.** Ein Klick auf
    einen Chip sollte den Platzhalter an der Cursorposition einsetzen — nicht am Ende.
    *Wo:* Editor → Variablen.
34. **Kein Warnhinweis bei einer Variable ohne Namen.** `{#{|a,b}#}` wird still zu
    Text. Eine Zeile beim Speichern. *Wo:* Editor.
35. **Notizen und Prompt sehen gleich aus.** Der eine wird kopiert, der andere nie —
    das darf man optisch nicht raten müssen. *Wo:* Editor → Notizen.

## F · Kopieren, Variablen, Hotkeys

36. **Der Variablen-Dialog merkt sich Werte, sagt es aber nicht.** Beim zweiten Mal
    steht plötzlich etwas im Feld. Ein „zuletzt verwendet" darüber. *Wo:* Variablen-Dialog.
37. **Kein „alle Felder leeren" im Variablen-Dialog.** Bei fünf gemerkten Werten
    löscht man fünf Mal. *Wo:* Variablen-Dialog.
38. **Der Kopier-Cooldown ist unsichtbar.** Zwei Sekunden lang passiert nichts und
    niemand weiß warum. Entweder kurz anzeigen oder ganz weglassen. *Wo:* Kopieren.
39. **Hotkeys sind pro Gerät und werden nicht exportiert — das steht nirgends.** Nach
    einem Rechnerwechsel sind sie weg, ohne Erklärung. *Wo:* Editor → Hotkey. - Natürlich alle einstellungen usw speichern! Prüfe alles daraufhin genau!
40. **Es gibt keine Liste aller vergebenen Hotkeys.** Man muss jeden Prompt öffnen,
    um zu sehen, was belegt ist. *Wo:* Einstellungen. in zeile für globalen hotkey als button "Alle Hotkeys" mit Unter Menü also popup wo alle aufgelistet sind mit filter, suche etc. 
42. **Die Zwischenablage-Variable ist die nützlichste und die unbekannteste.**
    „Prompt + was gerade markiert ist" ist der halbe Alltag. *Wo:* Variablen-Hilfe.

## G · Schwebende Buttons und Ordner

43. **Ein schwebender Button lässt sich nicht wegräumen, nur entfernen.** Ein „für
    jetzt ausblenden" (bis zum Neustart) fehlt. *Wo:* Pillen-Menü. - auch bei ordner
44. **Es gibt keine Übersicht der schwebenden Buttons.** wenn ein schwebender button warum auch immer außerhalb des monitors landet (zb nach neustart, programm fehler, was auch immer) dann soll dieser gelöscht werden sodass sich keine pillen im hintergrund stapeln können. Das soll aber nicht dafür gelten wenn ich eine pille an den rand ziehe oder sowas, erst wenn eine pille komplett garnicht zu sehen ist dann erst entfernen, die anderen sieht man ja und wenn man zu viele hätte würde einen das sowieso stören. (gilt natürlich auch für die ordner!)
45. **Pillen-Ordner sind ausgeschaltet ausgeliefert, obwohl sie das Problem aus 44
    lösen.** *Wo:* Expertenmenü. - standard an!
46. **Ein Ordner zeigt nicht, was drin ist, bevor man ihn öffnet.** Die 2×2-Vorschau
    hilft bei Bildern, bei Text-Pillen nicht. Zwei Namen als Tooltip. *Wo:* Ordner-Icon.

## H · Ansichten und Gruppen

49. **Ansichten und Ansichtsgruppen sind zwei Konzepte für eine Sache.** Wer nur fünf
    Ansichten hat, braucht keine Gruppen — für den ist die Gruppierung eine
    Falle beim Ziehen. Gruppen ab acht Ansichten anbieten, vorher nicht. *Wo:* Kopfzeile. - mache standard aus
50. **Es fehlt „Ansicht duplizieren".** Ein ähnliches Raster baut man sonst von Hand
    nach. *Wo:* Ansicht-Kontextmenü.
51. **Kein Weg, einen Prompt in mehreren Ansichten zu zeigen.** Das ist eine bewusste
    Entscheidung, aber sie überrascht — ein Satz beim Verschieben würde helfen.
    *Wo:* Ansichten. - Nein lasse es zu Prompts in mehreren ansichten anzuzeigen! (+ expertenmenü option dafür)
52. **Ctrl+Tab durch die Ansichten kennt niemand.** Es steht in keinem Tooltip.
    *Wo:* Kopfzeile. bei den ansichten
53. **Die Farbe einer Ansicht wirkt nur auf den Reiter.** Ein dünner farbiger Rand um
    das Raster würde beim Wechsel sofort zeigen, wo man ist. *Wo:* Raster. - nur als Expertenmenü option standard aus

## I · Kopierverlauf und Statistik

54. **Der Verlauf hat zwei Spalten, von denen eine selten interessiert.** „Meistgenutzt"
    ändert sich kaum; „zuletzt kopiert" ist der Alltag. Eine Spalte, umschaltbar.
    *Wo:* Kopierverlauf.
55. **Aus dem Verlauf kann man nicht bearbeiten.** Man sieht, dass ein Prompt oft
    kopiert wird, und kann ihn nicht von dort verbessern. *Wo:* Verlaufszeile.
56. **Die Statistik im Backup-Fenster gehört nicht dorthin.** Wer Backups verwaltet,
    will nicht wissen, welcher Prompt der längste ist. *Wo:* Backup-Fenster. -> stimmt, verschiebe das ins verlauf menü! mache es schön!
57. **„Kürzester Prompt" ist eine Zahl, die niemanden interessiert.** Streichen.
    *Wo:* Statistik.
58. **Kein Verlauf pro Tag.** „Diese Woche 40 Kopien" sagt mehr als eine Gesamtzahl.
    *Wo:* Statistik.
59. **Der Verlauf lässt sich nicht teilweise löschen.** Nur alles oder nichts —
    einzelne Einträge zu entfernen ist erst seit kurzem möglich, ein Zeitraum fehlt.
    *Wo:* Kopierverlauf. einzelne löschen im verlauf, zeiträume löschen im expertenmenü

## J · Backups

60. **Backups laufen im Hintergrund, ohne dass man es merkt.** Das ist gut — aber es
    gibt kein sichtbares „zuletzt gesichert vor 2 Stunden" außerhalb des Dialogs.
    Ein kleines Zeichen in der Kopfzeile. *Wo:* Kopfzeile. - zeige kurzen unauffälligen toast backup erstellt wenn ein backup erstellt wurde (auch bei auto backup, auto backup sollte bei start und dann nach intervall sichern)
61. **Ein Backup lässt sich nicht ansehen, nur wiederherstellen.** Wer nicht sicher
    ist, welches das richtige ist, muss raten. Eine Liste der enthaltenen Namen.
    *Wo:* Backup-Liste.
62. **„Intelligente Aufbewahrung" erklärt sich nicht.** Vier Zahlen für Tag/Woche/
    Monat/Jahr sind eine Fachantwort auf eine einfache Frage. Drei Vorgaben
    („sparsam / normal / viel") mit Feineinstellung dahinter. *Wo:* Backup-Fenster. -> mache wie bisher ein 2x2 raster zum auswählen sparsam, normal, viel, Erweitert; bei erweitert soll dann das alte tag monat etc menü erscheinen. bei diesen menü wechseln soll sich die größe des fensters nicht verstellen!
63. **Das Backup-Passwort ist eine Falle.** Wer es vergisst, verliert alles, und der
    Dialog sagt das nicht deutlich genug. *Wo:* Backup-Passwort. -> Sage es deutlicher!
64. **Kein Backup vor einer Wiederherstellung.** Genau dann wäre eines am wichtigsten.
    *Wo:* Wiederherstellen. -> Frage per popup ob man das machen will
65. **Der Ordner mit den Snapshots ist nicht erreichbar.** Ein „Im Explorer öffnen".
    *Wo:* Backup-Fenster.

## K · Einstellungen und Expertenmenü

66. **Das Expertenmenü ist inzwischen ein zweites Programm.** Sechs Reiter, über
    hundert Schalter. Das ist gewollt — aber ohne „zuletzt geändert" findet man nicht
    zurück zu dem, was man gestern umgestellt hat. *Wo:* Expertenmenü.
67. **Kein „nur geänderte anzeigen".** Wer wissen will, wie weit er vom Standard weg
    ist, muss alles durchsehen. *Wo:* Expertenmenü.
68. **Die Suche im Expertenmenü sucht nur in Namen.** Tooltips mitzudurchsuchen würde
    viel mehr finden. *Wo:* Expertenmenü → Suche.
69. **„(Standard Aus)" steht hinter dem Namen und liest sich wie Teil davon.**
    Als kleines Zeichen rechts wäre es ruhiger. *Wo:* Expertenmenü.
70. **Einstellungen exportieren/importieren fehlt.** Nach einer Neuinstallation baut
    man alles nach. *Wo:* Einstellungen. -> Alle Einstellungen sollen im Backup enthalten sein, immer wenn man ein Backup einließt soll ein ppup fenster kommen mit auswahl Daten, Einstellungen (beides vorausgewählt) und einem button wiederherstellen. So kann man beides auch separat wiederherstellen und alles ist im Backup gesichert!
71. **Es gibt kein Zurücksetzen pro Gruppe.** Nur alles auf einmal — das traut sich
    niemand. *Wo:* Expertenmenü. -> Füge unten in jeder kategorie button "Kategorie X zurücksetzen" mit bestätigung wie bei gesamt zurücksetzen
72. **Die Skalierungsregler sind zu fein.** Fünf Werte für Fenster, Modale, Icons,
    Composer, Primärknopf; eine Gesamtskalierung würde 90 % der Fälle abdecken.
    *Wo:* Expertenmenü → Aussehen. -> Mache das nur in normalen einstellungen, expertenmenü soll genau sein!

## L · Themes und Optik

74. **16 Themes sind zu viele.** Die Hälfte unterscheidet sich nur im Akzentton.
    Sechs gute plus der Editor wären ehrlicher. *Wo:* Themes. -> ja aber lasse ki enthusiast dabei (benenne es eventuell neu)
75. **Der Theme-Editor speichert erst am Ende.** Man probiert, gefällt es nicht, und
    hat keinen Weg zurück außer Abbrechen. Ein „Zurück zum Ausgangspunkt". *Wo:* Theme-Editor. -> überarbeite im theme editor auch die x buttons rechts die sehen noch etwas pixlig aus
76. **Kein Import/Export eines Themes.** Ein selbst gebautes Theme ist an den Rechner
    gebunden. *Wo:* Theme-Editor.

## M · Medien, Screenshots, Pipette

80. **Die Pipette kann nur eine Farbe.** Eine kleine Historie der letzten fünf Werte
    würde beim Abgleich zweier Farben sofort helfen. *Wo:* Pipette. -> am besten imn tooltip anzeigen oder so (sage mir was du am besten empfiehlst)
81. **Das Format (hex/rgb/hsl) wählt man vorher, nicht nachher.** Nach dem Klick ist
    es zu spät. Alle drei in den Toast, Klick wählt. *Wo:* Pipette.
83. **Eine PDF-Vorschau zeigt nur die erste Seite, ohne das zu sagen.** *Wo:* PDF-Kachel. -> Füge option bei bearbeiten, anlegen etc ein für preview seiten zahl

## Bens Idee
Füge ein feature hinzu, sodass man über das expertenmenü das umstellen kann (standard aus), wenn eingeschaltet soll man einen ordner wählen können (standard ist im normalen daten ordner ein unterordner "Prompts") in welchem alle Prompts also alle Buttons, texte, bilder, pdfs, etc. gespeichert sein sollen (quasi alle prompts liste), wenn möglich als .md datei pro prompt, mit allen daten zu diesem prompt zum ein/auslesen. Das heißt das tool soll alle prompts, kacheln etc. live in diesem ordner speichern und bei änderung durch den user oder zb claude diese direkt live neu einlesen. Somit soll ein Setup ermöglicht werden indem man claude anweisen kann diese dateien zu optimieren oder sonst was und das tool übernimmt diese änderungen direkt live und voll automatisch. Die normale Datenbank soll dabei beibehalten werden! Änderungen an den md dateien etc sollen quasi wie user änderungen behandelt werden also auch direkt in der datenbank angepasst werden. beide datenstränge sollen somit harmonisch zusammen arbeiten und claude hat freien zugriff auf die erstellung, bearbeitung, löschung von den prompt kacheln. Erstelle in github beschreibung für dieses feature eigene abteilung wo genau erklärt wird wie man claude oder anderen agents zugriff darauf geben kann. Claude soll somit quasi alle daten des tools (außer die einstellungen!) komplett anpassen können, auch neue kacheln erstellen an richtigem platz etc. für sachen die claude auch braucht dafür wie die ansichten soll es für diese auch mds geben worin dann name, farbe, position, inhalt etc etc gespeichert sind. alles am besten auch ordentlich mit unterordnern sortieren im ausgewählten ordner für die mds etc. Mache das feature gut und gründlich! - mit dieser option kann man auch punkt 90 beheben! Wenn man diesen ordner über drive etc synced sollten die prompts auf mehreren geräte auch gleich sein! Füge das ebenso als fall in die github beschreibung hinzu!

## O · Import, Export, Daten

88. **Beim Import gibt es keine Vorschau.** Man erfährt erst hinterher, wie viel
    dazugekommen ist. *Wo:* Import.
89. **Duplikate findet man nur über einen eigenen Dialog.** Sinnvoller wäre eine
    Warnung beim Anlegen. *Wo:* Editor.
90. **Der Datenordner lässt sich verschieben, aber nicht spiegeln.** Wer zwischen
    zwei Rechnern wechselt, hätte gern einen Ordner in der Cloud — mit klarer Warnung
    zu gleichzeitigem Zugriff. *Wo:* Einstellungen → Daten.
91. **Kein Papierkorb.** Ein gelöschter Prompt ist weg; das Backup ist der einzige Weg
    zurück und der ist grob. *Wo:* Löschen.

## P · Tray, Fenster, Systemverhalten

92. **Das Tray-Menü ist die schnellste Art zu kopieren und die unbekannteste.**
    Ein Hinweis beim ersten Minimieren. *Wo:* Tray.
93. **Nach dem Kopieren aus dem Tray gibt es keine Rückmeldung.** Man klickt und
    hofft. Ein kurzer Toast. *Wo:* Tray.
95. **Das Fenster merkt sich Größe und Position, aber nicht den Bildschirm.** Bei
    einem abgezogenen zweiten Monitor startet es außerhalb. *Wo:* Fenstergeometrie.
96. **Autostart startet sichtbar.** Wer es einschaltet, will es meistens im Tray.
    *Wo:* Einstellungen → Autostart.

## Q · Grundsätzliches

97. **Das Programm kann inzwischen sehr viel — und sagt es nirgends.** Eine einzige
    Seite „Was geht hier alles" mit zwölf Zeilen wäre wertvoller als zwanzig weitere
    Tooltips. *Wo:* Einstellungen → Hilfe. -> Lasse Tooltips da! (überarbeite und optimiere diese vielleicht nochmal!). Füge aber in normale einstellungen ein hilfe menü mit erklärung und überblick über alle funktionen. Diese Erklärungen etc. sollen auch als dokumentation im github repo liegen und über beschreibung verlinkt sein. Diese dokumentation soll jede funktion und jede einstellung genauestens erläutern! Alle erläuterungen kurz und präzise, alles suaber sortiert und kategorisiert so wie übersicht mit den wichtigsten punkten aus allen kategorien, claude soll in zukunft diese doku immer weiter anpassen und überarbeiten für alles neue. Diese doku soll NUR in englisch (fallback) und Deutsch verfügbar sein, keine weiteren sprachen (merke dir das auch!). Erläutere hier vorallem standard aus funktionen, zeige am besten kategorie "Schon gewusst?" mit den besten oder so. Denk immer daran bei jeder kleinsten änderung die dokumentation ebenfalls anzupassen!
100. **Es fehlt eine einzige globale Taste, die alles kann.** Ein Fenster, das aufgeht,
     tippen lässt und den passenden Prompt kopiert — das würde die Kacheln, die Pillen
     und das Tray-Menü für viele überflüssig machen. Das ist der größte Vorschlag der
     Liste und widerspricht bewusst einem Teil des heutigen Aufbaus. *Wo:* global. -> ändere dafür den globalen hotkey in den einstellungen, statt der prompt liste soll dieser ein neues schlankeres fenster alleine öffnen wo man schnell (fuzzy) suchen kann zum schnellen kopieren, standard hotkey dafür soll sein STRG+ALT+P
