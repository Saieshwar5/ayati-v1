"""Create a labelled synthetic delivery image; requires the existing Pillow tooling."""
import sys
from PIL import Image, ImageDraw
image = Image.new('RGB', (640, 360), '#f3f0e9')
draw = ImageDraw.Draw(image)
draw.text((24, 20), 'ASTER DISTRIBUTION - DELIVERY INSPECTION', fill='#202020', font_size=24)
draw.text((24, 56), 'Synthetic test image - Northstar Studio / 20 desks', fill='#505050', font_size=18)
draw.rounded_rectangle((100, 110, 540, 255), radius=8, fill='#bc9068', outline='#705138', width=4)
draw.rectangle((120, 255, 140, 310), fill='#555555')
draw.rectangle((500, 255, 520, 310), fill='#555555')
draw.line((515, 112, 539, 143, 515, 157, 539, 178), fill='#b62323', width=8)
draw.text((24, 327), 'Marked corner damage: human review required before closing the case.', fill='#8b1818', font_size=17)
image.save(sys.argv[1])
