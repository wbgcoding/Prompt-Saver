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
3. **Das Expertenmenü findet niemand von selbst.** Es steckt hinter den
   Einstellungen, und dort liegt der halbe Funktionsumfang. Ein Satz in den
   Einstellungen, der sagt, was dahinter liegt. *Wo:* Einstellungen.
4. **Kein Hinweis, dass Prompts lokal bleiben.** Genau das ist ein Verkaufsargument
   und steht nirgends im Programm. Eine Zeile in „Über". *Wo:* Einstellungen → Über.
5. **Die Standardansicht heißt wie ein Platzhalter.** Ein Name wie „Meine Prompts"
   trägt mehr als „Ansicht 1". *Wo:* erste Ansicht.

## B · Raster und Kacheln

6. **Eine Kachel zeigt nicht, wie lang ihr Prompt ist.** Zwei Zeilen oder zwei Seiten
   sehen identisch aus. Ein sehr dezenter Balken oder eine Zeichenzahl im Tooltip
   würde die Auswahl beschleunigen. *Wo:* Kachel.
7. **Kopieren gibt zu wenig zurück.** Die Kachel blinkt kurz. Bei einem langen Prompt
   wüsste man gern, dass wirklich alles kopiert wurde — „1.240 Zeichen kopiert" im
   Toast. *Wo:* Kopier-Feedback.
8. **Es fehlt „an dieselbe Stelle zurück".** Verschiebt man eine Kachel versehentlich,
   gibt es kein Rückgängig. Ein einmaliges Undo direkt nach dem Ablegen. *Wo:* Raster.
9. **Leere Zellen sind totes Gebiet.** Ein Doppelklick auf eine leere Zelle könnte
   direkt dort einen neuen Prompt anlegen, statt unten zu schreiben und dann zu
   ziehen. *Wo:* Raster.
10. **Die Rasterspalten-Steuerung oben ist ein Nischenwerkzeug.** Sie steht dauerhaft
    in der Kopfzeile und wird selten benutzt. Sie gehört in die Ansichts-Einstellungen
    — oder bleibt, aber nur solange man mit der Maus in der Kopfzeile ist. *Wo:*
    Kopfzeile (Schnellraster).
11. **Mehrfachauswahl gibt es nur in der Bibliothek.** Im Raster wäre sie genauso
    nützlich (mehrere Kacheln färben, verschieben, löschen). *Wo:* Raster.
12. **Kein „Prompt duplizieren".** Varianten eines Prompts baut man ständig; heute
    heißt das kopieren, neu anlegen, einfügen. *Wo:* Kachel-Kontextmenü.
13. **Die Kachelfarbe ist nur Dekoration.** Sie könnte mehr: nach Farbe im Raster
    sortieren oder alle einer Farbe auf einmal in eine Ansicht schieben. *Wo:* Farben.
14. **Kein Hinweis, wenn ein Prompt eine Variable enthält.** Man merkt es erst beim
    Kopieren, wenn ein Dialog aufgeht. Ein kleines Zeichen auf der Kachel. *Wo:* Kachel.
15. **Die Typ-Abzeichen (Bild, PDF, Datei) sind hilfreich, das Kettensymbol nicht.**
    Prompt-Verkettung nutzt kaum jemand dauerhaft; das Abzeichen kostet trotzdem Platz
    auf jeder Kachel. *Wo:* Kachel-Abzeichen.

## C · Bibliothek

16. **Die Bibliothek und das Raster fühlen sich wie zwei Programme an.** Wer in der
    Bibliothek etwas findet, will es meistens sofort kopieren — der Weg zurück ins
    Raster ist unnötig. Das funktioniert schon, ist aber nicht erkennbar.
    *Wo:* Bibliothek.
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
    Sitzung, danach Undo im Toast. *Wo:* Bibliothek → Löschen.
22. **Die Spaltenanzahl der Bibliothek ist eine Einstellung zu viel.** Sie steht in
    der Kopfzeile, im Expertenmenü und wirkt nur in der Bibliothek. Eine Stelle reicht.
    *Wo:* Bibliothek.

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
    Hotkey, Schrift, Größe, Verlauf — alles gleichzeitig. Zwei Reiter („Inhalt" und
    „Aussehen & Verhalten") würden ihn halbieren, ohne etwas zu entfernen.
    *Wo:* Editor.
29. **Die Zeichen-/Token-Anzeige interessiert die meisten nicht.** Sie ist eine
    Zahl ohne Handlung. Als Tooltip auf der Kachel genügt sie. *Wo:* Editor.
30. **Kein „Speichern und neu".** Wer zehn Prompts hintereinander anlegt, klickt jedes
    Mal durch dasselbe Fenster. *Wo:* Editor.
31. **Die Versionsgeschichte zeigt keinen Unterschied.** Man sieht alte Fassungen,
    aber nicht, was sich geändert hat. Ohne Diff ist der Wert klein. *Wo:* Editor → Verlauf.
32. **Das Schrift- und Größenpaar pro Kachel ist Überbau.** Es betrifft nur die Optik
    einer einzelnen Kachel und steht mitten im Editor. Gehört ins Kontextmenü der
    Kachel. *Wo:* Editor.
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
    einem Rechnerwechsel sind sie weg, ohne Erklärung. *Wo:* Editor → Hotkey.
40. **Es gibt keine Liste aller vergebenen Hotkeys.** Man muss jeden Prompt öffnen,
    um zu sehen, was belegt ist. *Wo:* Einstellungen.
41. **„Nach dem Kopieren einfügen" ist eine gefährliche Voreinstellung zum Verstecken.**
    Wer es versehentlich anlässt, schreibt in fremde Fenster. Es gehört sichtbar in die
    normalen Einstellungen, nicht ins Expertenmenü. *Wo:* Einstellungen.
42. **Die Zwischenablage-Variable ist die nützlichste und die unbekannteste.**
    „Prompt + was gerade markiert ist" ist der halbe Alltag. *Wo:* Variablen-Hilfe.

## G · Schwebende Buttons und Ordner

43. **Ein schwebender Button lässt sich nicht wegräumen, nur entfernen.** Ein „für
    jetzt ausblenden" (bis zum Neustart) fehlt. *Wo:* Pillen-Menü.
44. **Es gibt keine Übersicht der schwebenden Buttons.** Wer fünfzehn hat, weiß nicht
    mehr, welche wo sind, und einer kann hinter einem Fenster verschwinden. Eine Liste
    mit „alle einsammeln". *Wo:* Einstellungen.
45. **Pillen-Ordner sind ausgeschaltet ausgeliefert, obwohl sie das Problem aus 44
    lösen.** *Wo:* Expertenmenü.
46. **Ein Ordner zeigt nicht, was drin ist, bevor man ihn öffnet.** Die 2×2-Vorschau
    hilft bei Bildern, bei Text-Pillen nicht. Zwei Namen als Tooltip. *Wo:* Ordner-Icon.
47. **Kein Tastenweg zu den schwebenden Buttons.** Sie sind ein Mauswerkzeug —
    für ein Programm, dessen Kern das Tastatur-Kopieren ist, eine Lücke. *Wo:* Pillen.
48. **Die Größenvorgaben S/M/L und das freie Ziehen konkurrieren.** Nachdem man frei
    gezogen hat, ist keine Vorgabe mehr markiert und das sieht nach Fehler aus.
    *Wo:* Ordner-Menü.

## H · Ansichten und Gruppen

49. **Ansichten und Ansichtsgruppen sind zwei Konzepte für eine Sache.** Wer nur fünf
    Ansichten hat, braucht keine Gruppen — für den ist die Gruppierung eine
    Falle beim Ziehen. Gruppen ab acht Ansichten anbieten, vorher nicht. *Wo:* Kopfzeile.
50. **Es fehlt „Ansicht duplizieren".** Ein ähnliches Raster baut man sonst von Hand
    nach. *Wo:* Ansicht-Kontextmenü.
51. **Kein Weg, einen Prompt in mehreren Ansichten zu zeigen.** Das ist eine bewusste
    Entscheidung, aber sie überrascht — ein Satz beim Verschieben würde helfen.
    *Wo:* Ansichten.
52. **Ctrl+Tab durch die Ansichten kennt niemand.** Es steht in keinem Tooltip.
    *Wo:* Kopfzeile.
53. **Die Farbe einer Ansicht wirkt nur auf den Reiter.** Ein dünner farbiger Rand um
    das Raster würde beim Wechsel sofort zeigen, wo man ist. *Wo:* Raster.

## I · Kopierverlauf und Statistik

54. **Der Verlauf hat zwei Spalten, von denen eine selten interessiert.** „Meistgenutzt"
    ändert sich kaum; „zuletzt kopiert" ist der Alltag. Eine Spalte, umschaltbar.
    *Wo:* Kopierverlauf.
55. **Aus dem Verlauf kann man nicht bearbeiten.** Man sieht, dass ein Prompt oft
    kopiert wird, und kann ihn nicht von dort verbessern. *Wo:* Verlaufszeile.
56. **Die Statistik im Backup-Fenster gehört nicht dorthin.** Wer Backups verwaltet,
    will nicht wissen, welcher Prompt der längste ist. *Wo:* Backup-Fenster.
57. **„Kürzester Prompt" ist eine Zahl, die niemanden interessiert.** Streichen.
    *Wo:* Statistik.
58. **Kein Verlauf pro Tag.** „Diese Woche 40 Kopien" sagt mehr als eine Gesamtzahl.
    *Wo:* Statistik.
59. **Der Verlauf lässt sich nicht teilweise löschen.** Nur alles oder nichts —
    einzelne Einträge zu entfernen ist erst seit kurzem möglich, ein Zeitraum fehlt.
    *Wo:* Kopierverlauf.

## J · Backups

60. **Backups laufen im Hintergrund, ohne dass man es merkt.** Das ist gut — aber es
    gibt kein sichtbares „zuletzt gesichert vor 2 Stunden" außerhalb des Dialogs.
    Ein kleines Zeichen in der Kopfzeile. *Wo:* Kopfzeile.
61. **Ein Backup lässt sich nicht ansehen, nur wiederherstellen.** Wer nicht sicher
    ist, welches das richtige ist, muss raten. Eine Liste der enthaltenen Namen.
    *Wo:* Backup-Liste.
62. **„Intelligente Aufbewahrung" erklärt sich nicht.** Vier Zahlen für Tag/Woche/
    Monat/Jahr sind eine Fachantwort auf eine einfache Frage. Drei Vorgaben
    („sparsam / normal / viel") mit Feineinstellung dahinter. *Wo:* Backup-Fenster.
63. **Das Backup-Passwort ist eine Falle.** Wer es vergisst, verliert alles, und der
    Dialog sagt das nicht deutlich genug. *Wo:* Backup-Passwort.
64. **Kein Backup vor einer Wiederherstellung.** Genau dann wäre eines am wichtigsten.
    *Wo:* Wiederherstellen.
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
    man alles nach. *Wo:* Einstellungen.
71. **Es gibt kein Zurücksetzen pro Gruppe.** Nur alles auf einmal — das traut sich
    niemand. *Wo:* Expertenmenü.
72. **Die Skalierungsregler sind zu fein.** Fünf Werte für Fenster, Modale, Icons,
    Composer, Primärknopf; eine Gesamtskalierung würde 90 % der Fälle abdecken.
    *Wo:* Expertenmenü → Aussehen.
73. **„Vor Bildschirmaufnahmen verbergen" ist an und überrascht jeden.** Man kann das
    Programm nicht screenshotten und findet nicht heraus, warum. Der Tooltip muss das
    Wort „Screenshot" enthalten. *Wo:* Expertenmenü → Datenschutz.

## L · Themes und Optik

74. **16 Themes sind zu viele.** Die Hälfte unterscheidet sich nur im Akzentton.
    Sechs gute plus der Editor wären ehrlicher. *Wo:* Themes.
75. **Der Theme-Editor speichert erst am Ende.** Man probiert, gefällt es nicht, und
    hat keinen Weg zurück außer Abbrechen. Ein „Zurück zum Ausgangspunkt". *Wo:* Theme-Editor.
76. **Kein Import/Export eines Themes.** Ein selbst gebautes Theme ist an den Rechner
    gebunden. *Wo:* Theme-Editor.
77. **Das Gradient-Theme animiert dauerhaft.** Hübsch für zwei Minuten, danach
    Unruhe im Augenwinkel — und Strom. Abschaltbar. *Wo:* Theme „Verlauf".
78. **Der Dunkelmodus folgt dem System nicht automatisch, wenn ein festes Theme
    gewählt ist.** Das ist logisch, aber überrascht abends. *Wo:* Themes.

## M · Medien, Screenshots, Pipette

79. **Ein Screenshot landet sofort als Prompt.** Meistens will man ihn zuschneiden.
    Der Editor dafür ist geplant — bis dahin fehlt wenigstens „nochmal aufnehmen".
    *Wo:* Screenshot-Ergebnis.
80. **Die Pipette kann nur eine Farbe.** Eine kleine Historie der letzten fünf Werte
    würde beim Abgleich zweier Farben sofort helfen. *Wo:* Pipette.
81. **Das Format (hex/rgb/hsl) wählt man vorher, nicht nachher.** Nach dem Klick ist
    es zu spät. Alle drei in den Toast, Klick wählt. *Wo:* Pipette.
82. **Videos in Kacheln sind ein Nischenfall mit viel Code dahinter.** Ehrliche Frage,
    ob das Kachelraster der richtige Ort für Videowiedergabe ist. *Wo:* Medien.
83. **Eine PDF-Vorschau zeigt nur die erste Seite, ohne das zu sagen.** *Wo:* PDF-Kachel.

## N · Zwischenablage-Posteingang

84. **Der Posteingang und der Kopierverlauf sind schwer auseinanderzuhalten.** Zwei
    Symbole nebeneinander, beide mit Zwischenablage-Bezug. Ein Symbol mit zwei Reitern.
    *Wo:* Kopfzeile.
85. **Der Posteingang sammelt still.** Ohne Zähler merkt man nicht, dass etwas da ist.
    *Wo:* Kopfzeile.
86. **Kein „aus Posteingang direkt als Prompt mit Namen".** Heute übernimmt man den
    Text und benennt danach. *Wo:* Posteingang.

## O · Import, Export, Daten

87. **Export kann alles oder nichts.** Eine Auswahl (aktuelle Ansicht, Filter, Tag)
    fehlt. *Wo:* Einstellungen → Export.
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
94. **„Immer im Vordergrund" gilt fürs ganze Fenster.** Meistens will man nur die
    Bibliothek oben halten. *Wo:* Kopfzeile.
95. **Das Fenster merkt sich Größe und Position, aber nicht den Bildschirm.** Bei
    einem abgezogenen zweiten Monitor startet es außerhalb. *Wo:* Fenstergeometrie.
96. **Autostart startet sichtbar.** Wer es einschaltet, will es meistens im Tray.
    *Wo:* Einstellungen → Autostart.

## Q · Grundsätzliches

97. **Das Programm kann inzwischen sehr viel — und sagt es nirgends.** Eine einzige
    Seite „Was geht hier alles" mit zwölf Zeilen wäre wertvoller als zwanzig weitere
    Tooltips. *Wo:* Einstellungen → Hilfe.
98. **Zu viele Funktionen sind standardmäßig aus.** Wer nicht ins Expertenmenü geht,
    benutzt ein deutlich kleineres Programm als das, das installiert ist. Ein Profil
    („schlank / normal / alles") beim ersten Start. *Wo:* Erster Start.
99. **Suche gibt es an drei Stellen mit drei Verhalten** (Bibliothek, Verlauf,
    Expertenmenü). Eine gemeinsame Suche, die überall gleich funktioniert, wäre
    weniger zu lernen. *Wo:* überall.
100. **Es fehlt eine einzige globale Taste, die alles kann.** Ein Fenster, das aufgeht,
     tippen lässt und den passenden Prompt kopiert — das würde die Kacheln, die Pillen
     und das Tray-Menü für viele überflüssig machen. Das ist der größte Vorschlag der
     Liste und widerspricht bewusst einem Teil des heutigen Aufbaus. *Wo:* global.
