# 7. Troubleshooting

[Guide contents](../README.md) · [Download the complete PDF](https://github.com/csd113/Places/releases/download/v0.13.1/Beginners_Guide_to_Your_First_Places_Space.pdf)

In this chapter:

- [34. When something doesn’t behave as expected](#step-34)

<a id="step-34"></a>

## 34. When something doesn’t behave as expected

**Simple-mode limits:** Room creation opens a form rather than drawing a rectangle. The pillar, recessed floor and water tools also expose forms. We use them where they make the required shape or matching basin/water values clear. The rest of this guide uses the visual tools available in Simple; no step requires switching to Advanced.

| What you see | What to check |
| --- | --- |
| A tool seems stuck on the pointer | Press Esc to cancel it, then choose the tool again. |
| The wrong object changes when you pick a prop | A prop was still selected. Undo, click clear floor, then choose the new model and Place chosen prop. |
| A prop won’t place | Read the bottom message. Pan until the center of the view is over clear floor, then try again. Large props need more room. |
| A drag selects the floor instead of moving the object | Select the object again, wait for its colored handles, and drag a colored line. After Duplicate, use the new copy’s handles. |
| A room looks roofless | Ceilings are hidden from above. Look up from inside the first-person test to see the ceiling. |
| A new Room has no walls | That is expected. Draw its four walls with Build → Wall. |
| A wall won’t take a material | Select a solid wall face, confirm Properties says Wall, then open its Material button. This resets the catalog’s surface target. |
| A door blocks you while opening | Step back to give it room to swing, then walk through the center of the gap. |
| The water covers the deck | Check the water and basin share X, Z, width and depth. Water surface must be -0.1 and the bottom -0.4 for this example. |
| The game shows an older version | Save, rebuild, export again, copy the new package into Import folder and run Import levels. |



[Back to chapter contents](#7-troubleshooting)

---

[← Build, export and play](06-play.md) · [Guide contents →](../README.md)
